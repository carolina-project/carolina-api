use std::{hash::Hash, path::PathBuf, str::FromStr, sync::Arc};

use dashmap::DashMap;
use fxhash::FxHashMap;
use onebot_connect_interface::app::{AppDyn, MessageSource, OBApp, OBAppProvider, RecvMessage};
use rand::Rng;
use tokio::sync::RwLock;

use crate::{context::*, CarolinaPlugin, PluginInfo, PluginInfoBuilder};

#[derive(Default, Debug)]
pub struct EventMapper {
    type2uid: DashMap<String, DashMap<String, Vec<PluginRid>>>,
    uid2type: DashMap<PluginRid, Vec<(String, String)>>,
}

impl EventMapper {
    pub fn register(&self, types: Vec<(String, String)>, rid: PluginRid) {
        for (ty, detail_ty) in &types {
            self.type2uid
                .entry(ty.clone())
                .or_default()
                .entry(detail_ty.clone())
                .or_default()
                .push(rid);
        }

        self.uid2type.insert(rid, types);
    }

    pub fn filter_plugins(
        &self,
        ty: impl AsRef<str>,
        detail_ty: impl AsRef<str>,
    ) -> Vec<PluginRid> {
        self.type2uid
            .get(ty.as_ref())
            .and_then(|map| map.get(detail_ty.as_ref()).map(|r| r.clone()))
            .unwrap_or_default()
    }
}

pub struct DirConfig {
    config_path: PathBuf,
    data_path: PathBuf,
}
impl DirConfig {
    pub fn new(config: Option<PathBuf>, data: Option<PathBuf>) -> Self {
        let config_path = config.unwrap_or_else(|| {
            dirs::config_dir()
                .unwrap_or_else(|| PathBuf::from_str(".").unwrap())
                .join("config")
        });
        let data_path = data.unwrap_or_else(|| {
            dirs::data_dir()
                .unwrap_or_else(|| PathBuf::from_str(".").unwrap())
                .join("data")
        });

        DirConfig {
            config_path,
            data_path,
        }
    }
}

pub struct GlobalContextInner<P: CarolinaPlugin> {
    plugin_rid_map: RwLock<FxHashMap<PluginRid, P>>,
    plugin_id2rid: DashMap<String, PluginRid>,
    plugin_rid2info: DashMap<PluginRid, PluginInfo>,
    event_mapper: EventMapper,

    shared_apps: DashMap<AppRid, Box<dyn AppDyn + Sync>>,
    dir_config: DirConfig,
}

pub struct GlobalContextImpl<P: CarolinaPlugin> {
    inner: Arc<GlobalContextInner<P>>,
}

pub struct GlobalDestructed<P: CarolinaPlugin> {
    pub plugins: FxHashMap<PluginRid, (PluginInfo, P)>,
    pub shared_apps: DashMap<AppRid, Box<dyn AppDyn + Sync>>,
}

impl<P: CarolinaPlugin> GlobalContextImpl<P> {
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
            }
            .into(),
        }
    }

    pub async fn append_plugin(&self, plugin: P) -> PluginRid {
        let mut map = self.inner.plugin_rid_map.write().await;
        let mut rid: PluginRid;
        let mut rng = rand::thread_rng();
        loop {
            rid = rng.gen::<u64>().into();
            if !map.contains_key(&rid) {
                break;
            }
        }
        let info = plugin.info();
        self.inner
            .event_mapper
            .register(plugin.register_events().await, rid);
        map.insert(rid, plugin);
        self.inner.plugin_id2rid.insert(info.id.clone(), rid);
        self.inner.plugin_rid2info.insert(rid, info);

        rid
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
                    PluginInfoBuilder::new("unknown").build()
                }
            };
            plugins.insert(rid, (info, map.remove(&rid).unwrap()));
        }
        let apps = DashMap::default();
        for ele in self.inner.shared_apps.iter() {
            apps.insert(*ele.key(), ele.value().clone_app() as _);
        }

        GlobalDestructed {
            plugins,
            shared_apps: apps,
        }
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

impl<P: CarolinaPlugin> GlobalContext for GlobalContextImpl<P> {
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

    async fn call_plugin_api(&self, src: PluginRid, call: APICall) -> APIResult {
        match self.inner.plugin_rid_map.read().await.get(&call.target) {
            Some(plug) => plug.handle_api_call(src, call).await,
            None => Err(APIError::PluginNotFound(call.target)),
        }
    }

    fn get_config_dir(&self, rid: Option<PluginRid>) -> crate::BResult<PathBuf> {
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

    fn get_data_dir(&self, rid: Option<PluginRid>) -> crate::BResult<PathBuf> {
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

    fn register_connect(
        &self,
        plugin_rid: PluginRid,
        mut provider: impl OBAppProvider,
        mut source: impl MessageSource,
    ) {
        let app_id = rand_u64(&self.inner.shared_apps);
        if !provider.use_event_context() {
            match provider.provide() {
                Ok(app) => {
                    self.inner.shared_apps.insert(app_id, Box::new(app));
                }
                Err(e) => {
                    log::error!("app provider error({plugin_rid}): {e}")
                }
            }
        }

        let inner = self.inner.clone();
        tokio::spawn(async move {
            while let Some(msg) = source.poll_message().await {
                match msg {
                    RecvMessage::Event(event) => {
                        if provider.use_event_context() {
                            provider.set_event_context(&event);
                        }
                        let mut app = match provider.provide() {
                            Ok(app) => app,
                            Err(e) => {
                                log::error!("app provider error({plugin_rid}): {e}");
                                continue;
                            }
                        };

                        let plugins = inner
                            .event_mapper
                            .filter_plugins(&event.event.r#type, &event.event.detail_type);
                        for ele in plugins {
                            let map = inner.plugin_rid_map.read().await;
                            if let Some(plugin) = map.get(&ele) {
                                let handle_res = plugin
                                    .handle_event(
                                        event.clone(),
                                        EventContext::new(app_id, OBApp::clone_app(&app)),
                                    )
                                    .await;
                                if let Err(e) = handle_res {
                                    log::error!("plugin handle error({ele}): {e}");
                                }
                            } else {
                                log::error!("unexpected error, plugin not found({ele})");
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
        });
    }
}
