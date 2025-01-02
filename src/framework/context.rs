use super::*;
use crate::*;
use util::*;

use std::{hash::Hash, path::PathBuf, sync::Arc};

use dashmap::DashMap;
use fxhash::FxHashMap;
use onebot_connect_interface::app::{AppDyn, MessageSource, OBApp, OBAppProvider, RecvMessage};
use rand::Rng;
use tokio::{fs, sync::RwLock};

pub struct GlobalContextInner<P: CarolinaPlugin> {
    plugin_rid_map: RwLock<FxHashMap<PluginRid, (bool, UnsafePluginWrapper<P>)>>,
    plugin_id2rid: DashMap<String, PluginRid>,
    plugin_rid2info: DashMap<PluginRid, PluginInfo>,
    event_mapper: EventMapper,

    shared_apps: DashMap<AppRid, Box<dyn AppDyn + Sync>>,
    dir_config: DirConfig,
    running: Completed,
}

pub struct GlobalContextImpl<P: CarolinaPlugin> {
    inner: Arc<GlobalContextInner<P>>,
}

impl<P: CarolinaPlugin> Clone for GlobalContextImpl<P> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

pub struct GlobalDestructed<P: CarolinaPlugin> {
    pub plugins: FxHashMap<PluginRid, (PluginInfo, P)>,
    pub shared_apps: FxHashMap<AppRid, Box<dyn AppDyn + Sync>>,
}

impl<P: CarolinaPlugin + 'static> GlobalContextImpl<P> {
    pub fn new(dir_config: DirConfig) -> Self {
        fn default<T: Default>() -> T {
            T::default()
        }

        Self {
            inner: GlobalContextInner {
                shared_apps: default(),
                plugin_rid_map: default(),
                plugin_id2rid: default(),
                plugin_rid2info: default(),
                event_mapper: default(),
                dir_config,
                running: Completed::default(),
            }
            .into(),
        }
    }

    pub async fn init_plugin(
        &self,
        mut plugin: P,
        info: PluginInfo,
        rt: Option<Runtime>,
    ) -> StdResult<PluginRid> {
        let mut map = self.inner.plugin_rid_map.write().await;
        let mut rid: PluginRid;
        let mut rng = rand::thread_rng();
        loop {
            rid = rng.gen::<u64>().into();
            if !map.contains_key(&rid) {
                break;
            }
        }
        let id = info.id.clone();
        self.inner.plugin_id2rid.insert(id.clone(), rid);
        self.inner.plugin_rid2info.insert(rid, info);
        fs::create_dir_all(self.inner.dir_config.config_path.join(id.as_str())).await?;
        fs::create_dir_all(self.inner.dir_config.data_path.join(id.as_str())).await?;

        let is_rt = rt.is_some();
        if let Err(e) = plugin.init(PluginContext::new(rid, self.clone(), rt)).await {
            self.inner.plugin_id2rid.remove(&id);
            self.inner.plugin_rid2info.remove(&rid);
            return Err(e);
        }
        let subscribed = plugin.subscribe_events().await;
        log::debug!("[{id}] Subscribed event: {subscribed:?}");
        self.inner.event_mapper.subscribe(subscribed, rid);
        map.insert(rid, (is_rt, plugin.into()));

        Ok(rid)
    }

    pub async fn post_init(
        &self,
        rt_make: impl Fn() -> Runtime,
        on_err: impl Fn(PluginRid, String, Box<dyn std::error::Error>),
    ) {
        for (rid, (is_rt, ele)) in self.inner.plugin_rid_map.write().await.iter_mut() {
            let id = self
                .inner
                .plugin_rid2info
                .get(rid)
                .map(|r| r.id.clone())
                .unwrap_or_else(|| "unknown".into());

            log::info!("post-initializing plugin: {id}({rid})");
            if let Err(e) = ele
                .post_init(PluginContext::new(
                    *rid,
                    self.clone(),
                    if *is_rt { Some(rt_make()) } else { None },
                ))
                .await
            {
                on_err(*rid, id, e);
            }
        }

        self.inner.running.complete();
    }

    /// Destruct global context for deinitiialization.
    pub async fn destruct(self) -> GlobalDestructed<P> {
        let mut plugins: FxHashMap<PluginRid, (PluginInfo, P)> = FxHashMap::default();

        let keys: Vec<_> = self
            .inner
            .plugin_rid_map
            .read()
            .await
            .keys()
            .copied()
            .collect();
        let mut map = self.inner.plugin_rid_map.write().await;
        for rid in keys.into_iter() {
            let info = match self.inner.plugin_rid2info.remove(&rid) {
                Some(info) => info.1,
                None => {
                    log::error!("cannot find plugin info({rid})");
                    continue;
                }
            };
            plugins.insert(rid, (info, map.remove(&rid).unwrap().1.into_inner()));
        }
        let mut apps = FxHashMap::default();
        let keys = self
            .inner
            .shared_apps
            .iter()
            .map(|ele| *ele.key())
            .collect::<Vec<_>>();
        for ele in keys {
            let Some((_, app)) = self.inner.shared_apps.remove(&ele) else {
                continue;
            };
            apps.insert(ele, app);
        }

        GlobalDestructed {
            plugins,
            shared_apps: apps,
        }
    }

    pub fn get_rid_map(&self) -> &DashMap<PluginRid, PluginInfo> {
        &self.inner.plugin_rid2info
    }
}

fn rand_u64<K: Into<u64> + From<u64> + Hash + Eq + Clone, V>(map: &DashMap<K, V>) -> K {
    let mut rid: u64;
    let mut rng = rand::thread_rng();
    loop {
        rid = rng.gen();
        let key: K = rid.into();
        if !map.contains_key(&key) {
            return key;
        }
    }
}

impl<PL: CarolinaPlugin + 'static> GlobalContext for GlobalContextImpl<PL> {
    fn get_shared_app(&self, id: AppRid) -> Option<Box<dyn AppDyn>> {
        self.inner.shared_apps.get(&id).map(|r| r.clone_app())
    }

    fn get_plugin_rid(&self, id: &str) -> Option<PluginRid> {
        self.inner.plugin_id2rid.get(id).map(|r| *r)
    }

    fn get_plugin_id(&self, rid: impl Into<PluginRid>) -> Option<String> {
        self.inner
            .plugin_rid2info
            .get(&rid.into())
            .map(|r| r.id.clone())
    }

    async fn call_plugin_api(&self, src: PluginRid, target: PluginRid, call: APICall) -> APIResult {
        if !self.inner.running.is_completed() && src == target {
            return Err(APIError::other(
                "initialization hasn't finish, cannot call self",
            ));
        }

        match self.inner.plugin_rid_map.read().await.get(&target) {
            Some(plug) => plug.1.handle_api_call(src, call).await,
            None => Err(APIError::PluginNotFound(target)),
        }
    }

    fn get_config_dir(&self, rid: Option<PluginRid>) -> crate::StdResult<PathBuf> {
        match rid {
            Some(rid) => {
                let id = self
                    .inner
                    .plugin_rid2info
                    .get(&rid)
                    .ok_or_else(|| format!("cannot find plugin id by rid({rid})"))?;
                Ok(self.inner.dir_config.config_path.join(id.id.as_str()))
            }
            None => Ok(self.inner.dir_config.config_path.clone()),
        }
    }

    fn get_data_dir(&self, rid: Option<PluginRid>) -> crate::StdResult<PathBuf> {
        match rid {
            Some(rid) => {
                let id = self
                    .inner
                    .plugin_rid2info
                    .get(&rid)
                    .ok_or_else(|| format!("cannot find plugin id by rid({rid})"))?;
                Ok(self.inner.dir_config.data_path.join(id.id.as_str()))
            }
            None => Ok(self.inner.dir_config.data_path.clone()),
        }
    }

    fn register_connect<P, S>(&self, plugin_rid: PluginRid, mut provider: P, mut source: S)
    where
        P: OBAppProvider<Output: 'static> + 'static,
        S: MessageSource + 'static,
    {
        async fn handle_msg<
            P: OBAppProvider<Output: 'static> + 'static,
            PLG: CarolinaPlugin + 'static,
        >(
            plugin_rid: PluginRid,
            msg: RecvMessage,
            provider: &mut P,
            inner: Arc<GlobalContextInner<PLG>>,
            app_id: AppRid,
        ) {
            match msg {
                RecvMessage::Event(event) => {
                    if provider.use_event_context() {
                        provider.set_event_context(&event);
                    }
                    let mut app = match provider.provide() {
                        Ok(app) => app,
                        Err(e) => {
                            log::error!("app provider error({plugin_rid}): {e}");
                            return;
                        }
                    };

                    let plugins = inner
                        .event_mapper
                        .filter_plugins(&event.event.r#type, &event.event.detail_type);
                    for ele in plugins {
                        let map = inner.plugin_rid_map.read().await;
                        let Some(plugin) = map.get(&ele) else {
                            log::error!("unexpected error, plugin not found({ele})");
                            continue;
                        };

                        let handle_res = plugin
                            .1
                            .handle_event(
                                event.clone(),
                                EventContext::new(app_id, OBApp::clone_app(&app)),
                            )
                            .await;
                        if let Err(e) = handle_res {
                            log::error!("plugin handle error({ele}): {e}");
                        }
                    }

                    if let Err(e) = OBApp::release(&mut app).await {
                        log::error!("app release error({plugin_rid} -> {app_id}): {e}");
                    }
                }
                RecvMessage::Close(close) => {
                    log::info!("Connection closed: {close:?}")
                }
            }
        }

        let wait = self.inner.running.wait();
        let inner = self.inner.clone();
        tokio::spawn(async move {
            wait.await;

            let app_rid = rand_u64(&inner.shared_apps);
            if !provider.use_event_context() {
                match provider.provide() {
                    Ok(app) => {
                        inner.shared_apps.insert(app_rid, Box::new(app));
                    }
                    Err(e) => {
                        log::error!("app provider error({plugin_rid}): {e}")
                    }
                }
            }

            let inner = inner.clone();
            log::info!("{plugin_rid:?} registerd OneBot app: {app_rid:?}");
            tokio::spawn(async move {
                while let Some(msg) = source.poll_message().await {
                    handle_msg(plugin_rid, msg, &mut provider, inner.clone(), app_rid).await
                }
            });
        });
    }
}