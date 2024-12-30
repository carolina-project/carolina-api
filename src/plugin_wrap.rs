use crate::*;
use tokio::runtime as tok_rt;

use std::{cell::UnsafeCell, error::Error as ErrTrait, fmt::Display, sync::Arc};

struct UnsafePlugin<P: CarolinaPluginDyn>(Arc<UnsafeCell<P>>);
unsafe impl<P: CarolinaPluginDyn> Sync for UnsafePlugin<P> {}
unsafe impl<P: CarolinaPluginDyn> Send for UnsafePlugin<P> {}

impl<P: CarolinaPluginDyn> Clone for UnsafePlugin<P> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<P: CarolinaPluginDyn> Deref for UnsafePlugin<P> {
    type Target = P;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.0.get() }
    }
}
impl<P: CarolinaPluginDyn> DerefMut for UnsafePlugin<P> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.0.get() }
    }
}
impl<P: CarolinaPluginDyn> UnsafePlugin<P> {
    fn new(plug: P) -> Self {
        Self(Arc::new(UnsafeCell::new(plug)))
    }
}

pub struct DynPlugin<P: CarolinaPlugin> {
    plugin: UnsafePlugin<P>,
    async_rt: tok_rt::Runtime,
}

impl<P: CarolinaPlugin> DynPlugin<P> {
    pub fn new(plug: P) -> Self {
        Self {
            plugin: UnsafePlugin::new(plug),
            async_rt: tok_rt::Builder::new_multi_thread()
                .worker_threads(2)
                .build()
                .unwrap(),
        }
    }
}

#[derive(Debug)]
struct StringError(String);
impl StringError {
    fn boxed<T: Display>(msg: T) -> Box<dyn ErrTrait + Send> {
        Box::new(Self(msg.to_string()))
    }
}

impl Display for StringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl ErrTrait for StringError {}

impl<P: CarolinaPlugin> CarolinaPlugin for DynPlugin<P> {
    fn info(&self) -> PluginInfo {
        let _guard = self.async_rt.enter();
        self.plugin.info()
    }

    #[allow(unused)]
    fn init<G: GlobalContext>(
        &mut self,
        context: PluginContext<G>,
    ) -> impl Future<Output = BResult<()>> + Send + '_ {
        let mut plugin = self.plugin.clone();
        async move {
            self.async_rt
                .spawn(async move { plugin.init(context).await.map_err(StringError::boxed) })
                .await?
                .map_err(|e| e as _)
        }
    }

    #[allow(unused)]
    fn post_init<G: GlobalContext>(
        &mut self,
        context: PluginContext<G>,
    ) -> impl Future<Output = Result<(), Box<dyn std::error::Error>>> + Send + '_ {
        let mut plugin = self.plugin.clone();
        async move {
            self.async_rt
                .spawn(async move { plugin.post_init(context).await.map_err(StringError::boxed) })
                .await?
                .map_err(|e| e as _)
        }
    }

    fn subscribe_events(&self) -> impl Future<Output = Vec<(String, Option<String>)>> + Send + '_ {
        let _guard = self.async_rt.enter();
        self.plugin.subscribe_events()
    }

    #[allow(unused)]
    fn handle_event<EC>(
        &self,
        event: RawEvent,
        context: EC,
    ) -> impl Future<Output = Result<(), Box<dyn std::error::Error>>> + Send + '_
    where
        EC: EventContextTrait + Send + 'static,
    {
        let mut plugin = self.plugin.clone();
        async move {
            self.async_rt
                .spawn(async move {
                    plugin
                        .handle_event(event, context)
                        .await
                        .map_err(StringError::boxed)
                })
                .await?
                .map_err(|e| e as _)
        }
    }

    #[allow(unused)]
    fn handle_api_call(
        &self,
        src: PluginRid,
        call: APICall,
    ) -> impl Future<Output = APIResult> + Send + '_ {
        let plugin = self.plugin.clone();
        async move {
            self.async_rt
                .spawn(async move { plugin.handle_api_call(src, call).await })
                .await
                .map_err(APIError::other)?
        }
    }

    fn deinit(
        &mut self,
    ) -> impl Future<Output = Result<(), Box<dyn std::error::Error>>> + Send + '_ {
        let mut plugin = self.plugin.clone();
        async move {
            self.async_rt
                .spawn(async move { plugin.deinit().await.map_err(StringError::boxed) })
                .await?
                .map_err(|e| e as _)
        }
    }
}
