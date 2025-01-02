use crate::*;
use tokio::runtime as tok_rt;

use std::future::Future;
use std::{error::Error as ErrTrait, fmt::Display};
use std::ops::{Deref, DerefMut};

struct UnsafePlugin<P: CarolinaPlugin>(P);

impl<P: CarolinaPlugin> UnsafePlugin<P> {
    fn into_inner(self) -> P {
        self.0
    }
}

struct UnsafePluginMutRef<P: CarolinaPlugin>(*mut P);
struct UnsafePluginRef<P: CarolinaPlugin>(*const P);

impl<P: CarolinaPlugin> Deref for UnsafePluginRef<P> {
    type Target = P;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.0 }
    }
}

impl<P: CarolinaPlugin> Deref for UnsafePluginMutRef<P> {
    type Target = P;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.0 }
    }
}
impl<P: CarolinaPlugin> DerefMut for UnsafePluginMutRef<P> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.0 }
    }
}

unsafe impl<P: CarolinaPlugin> Sync for UnsafePluginRef<P> {}
unsafe impl<P: CarolinaPlugin> Send for UnsafePluginRef<P> {}
unsafe impl<P: CarolinaPlugin> Sync for UnsafePluginMutRef<P> {}
unsafe impl<P: CarolinaPlugin> Send for UnsafePluginMutRef<P> {}

impl<P: CarolinaPlugin> UnsafePlugin<P> {
    fn new(plug: P) -> Self {
        Self(plug)
    }

    fn as_ref(&self) -> UnsafePluginRef<P> {
        UnsafePluginRef(&self.0 as *const _)
    }

    fn as_ref_mut(&mut self) -> UnsafePluginMutRef<P> {
        UnsafePluginMutRef(&mut self.0 as *mut _)
    }
}

pub struct DynPlugin<P: CarolinaPlugin + 'static> {
    plugin: UnsafePlugin<P>,
    async_rt: tok_rt::Runtime,
}

impl<P: CarolinaPlugin> DynPlugin<P> {
    pub fn new(plug: P) -> Self {
        Self {
            plugin: UnsafePlugin::new(plug),
            async_rt: tok_rt::Builder::new_multi_thread().build().unwrap(),
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
        self.plugin.as_ref().info()
    }

    #[allow(unused)]
    async fn init<G: GlobalContext>(&mut self, context: PluginContext<G>) -> StdResult<()> {
        let mut plugin = self.plugin.as_ref_mut();
        self.async_rt
            .spawn(async move { plugin.init(context).await.map_err(StringError::boxed) })
            .await?
            .map_err(|e| e as _)
    }

    #[allow(unused)]
    async fn post_init<G: GlobalContext>(
        &mut self,
        context: PluginContext<G>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut plugin = self.plugin.as_ref_mut();
        self.async_rt
            .spawn(async move { plugin.post_init(context).await.map_err(StringError::boxed) })
            .await?
            .map_err(|e| e as _)
    }

    fn subscribe_events(
        &mut self,
    ) -> impl Future<Output = Vec<(String, Option<String>)>> + Send + '_ {
        let mut plugin = self.plugin.as_ref_mut();
        async move {
            self.async_rt
                .spawn(async move { plugin.subscribe_events().await })
                .await
                .unwrap()
        }
    }

    async fn handle_event<EC>(
        &self,
        event: RawEvent,
        context: EC,
    ) -> Result<(), Box<dyn std::error::Error>>
    where
        EC: EventContextTrait + Send + 'static,
    {
        let plugin = self.plugin.as_ref();
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

    async fn handle_api_call(&self, src: PluginRid, call: APICall) -> APIResult {
        let plugin = self.plugin.as_ref();
        self.async_rt
            .spawn(async move { plugin.handle_api_call(src, call).await })
            .await
            .map_err(APIError::other)?
    }

    async fn deinit(self) -> Result<(), Box<dyn std::error::Error>> {
        let DynPlugin { plugin, async_rt } = self;
        async_rt
            .spawn(async move { plugin.into_inner().deinit().await.map_err(StringError::boxed) })
            .await?
            .map_err(|e| e as _)
    }
}
