use std::{future::Future, ops::Deref, path::PathBuf, pin::Pin, sync::Arc};

use crate::StdResult;

use super::*;
use call::*;
use onebot_connect_interface::{
    app::{AppDyn, AppProviderDyn, MessageSource, MessageSourceDyn, OBApp, OBAppProvider},
    value::Value,
};

pub trait EventContextTrait {
    type App: OBApp + 'static;

    fn app(&self) -> &Self::App;
    fn app_marker(&self) -> AppRid;
    fn into_inner(self) -> (Self::App, AppRid);
}

/// Event context for static dispatching.
pub struct EventContext<A: OBApp + 'static> {
    marker: AppRid,
    app: A,
}
impl<A: OBApp> EventContext<A> {
    pub fn new(marker: AppRid, app: A) -> Self {
        Self { marker, app }
    }
}
impl<A: OBApp> EventContextTrait for EventContext<A> {
    type App = A;

    fn app(&self) -> &Self::App {
        &self.app
    }

    fn app_marker(&self) -> AppRid {
        self.marker
    }

    fn into_inner(self) -> (Self::App, AppRid) {
        (self.app, self.marker)
    }
}

/// Event context for dynamic dispatching.
pub struct DynEventContext {
    app_uid: AppRid,
    app: Box<dyn AppDyn>,
}
impl<A: OBApp + 'static> From<(A, AppRid)> for DynEventContext {
    fn from(app: (A, AppRid)) -> Self {
        Self::new(app.0, app.1)
    }
}
impl DynEventContext {
    fn new(app: impl AppDyn + 'static, app_id: AppRid) -> Self {
        Self {
            app_uid: app_id,
            app: Box::new(app),
        }
    }
}

impl EventContextTrait for DynEventContext {
    type App = Box<dyn AppDyn>;

    fn app(&self) -> &Self::App {
        &self.app
    }

    fn app_marker(&self) -> AppRid {
        self.app_uid
    }

    fn into_inner(self) -> (Self::App, AppRid) {
        (self.app, self.app_uid)
    }
}

pub trait GlobalContext: Send + Sync {
    fn get_shared_app(&self, id: AppRid) -> Option<Box<dyn AppDyn>>;

    fn get_plugin_rid(&self, id: &str) -> Option<PluginRid>;

    fn get_plugin_id(&self, rid: impl Into<PluginRid>) -> Option<String>;

    fn call_plugin_api(
        &self,
        src: PluginRid,
        target: PluginRid,
        call: APICall,
    ) -> impl Future<Output = APIResult> + Send + '_;

    fn register_connect<F, FR, P, S>(
        &self,
        rid: PluginRid,
        provider: P,
        source: S,
        close_callback: F,
    ) where
        P: OBAppProvider<Output: 'static> + 'static,
        S: MessageSource + 'static,
        F: FnOnce() -> FR + Send + 'static,
        FR: Future<Output = StdResult<()>> + Send + 'static;

    fn get_config_dir(&self, rid: Option<PluginRid>) -> StdResult<PathBuf>;

    fn get_data_dir(&self, rid: Option<PluginRid>) -> StdResult<PathBuf>;
}

pub trait GlobalContextDyn: Send + Sync {
    fn get_shared_app(&self, id: AppRid) -> Option<Box<dyn AppDyn>>;

    fn get_plugin_rid(&self, id: &str) -> Option<PluginRid>;

    fn get_plugin_id(&self, rid: PluginRid) -> Option<String>;

    fn call_plugin_api(
        &self,
        src: PluginRid,
        target: PluginRid,
        call: APICall,
    ) -> Pin<Box<dyn Future<Output = APIResult> + Send + '_>>;

    fn register_connect(
        &self,
        rid: PluginRid,
        provider: Box<dyn AppProviderDyn>,
        source: Box<dyn MessageSourceDyn>,
        close_callback: BoxedCallbackFn<'static>,
    );

    fn get_config_dir(&self, rid: Option<PluginRid>) -> StdResult<PathBuf>;

    fn get_data_dir(&self, rid: Option<PluginRid>) -> StdResult<PathBuf>;
}

// For parameter passing
impl<'a> GlobalContext for Box<dyn GlobalContextDyn + 'a> {
    fn get_shared_app(&self, id: AppRid) -> Option<Box<dyn AppDyn>> {
        self.deref().get_shared_app(id)
    }

    fn get_plugin_rid(&self, id: &str) -> Option<PluginRid> {
        self.deref().get_plugin_rid(id)
    }

    fn get_plugin_id(&self, rid: impl Into<PluginRid>) -> Option<String> {
        self.deref().get_plugin_id(rid.into())
    }

    fn call_plugin_api(
        &self,
        src: PluginRid,
        target: PluginRid,
        call: APICall,
    ) -> impl Future<Output = APIResult> + Send + '_ {
        self.deref().call_plugin_api(src, target, call)
    }

    fn register_connect<F, FR, P, S>(
        &self,
        rid: PluginRid,
        provider: P,
        source: S,
        close_callback: F,
    ) where
        P: OBAppProvider<Output: 'static> + 'static,
        S: MessageSource + 'static,
        F: FnOnce() -> FR + Send + 'static,
        FR: Future<Output = StdResult<()>> + Send + 'static,
    {
        self.deref().register_connect(
            rid,
            Box::new(provider),
            Box::new(source),
            boxed_async_cb(close_callback),
        );
    }

    fn get_config_dir(&self, rid: Option<PluginRid>) -> StdResult<PathBuf> {
        self.deref().get_config_dir(rid)
    }

    fn get_data_dir(&self, rid: Option<PluginRid>) -> StdResult<PathBuf> {
        self.deref().get_data_dir(rid)
    }
}

impl<T: GlobalContext> GlobalContextDyn for T {
    fn get_shared_app(&self, id: AppRid) -> Option<Box<dyn AppDyn>> {
        self.get_shared_app(id).map(|a| Box::new(a) as _)
    }

    fn get_plugin_rid(&self, id: &str) -> Option<PluginRid> {
        self.get_plugin_rid(id)
    }

    fn get_plugin_id(&self, rid: PluginRid) -> Option<String> {
        self.get_plugin_id(rid)
    }

    fn call_plugin_api(
        &self,
        src: PluginRid,
        target: PluginRid,
        call: APICall,
    ) -> Pin<Box<dyn Future<Output = APIResult> + Send + '_>> {
        Box::pin(self.call_plugin_api(src, target, call))
    }

    fn register_connect(
        &self,
        rid: PluginRid,
        provider: Box<dyn AppProviderDyn>,
        source: Box<dyn MessageSourceDyn>,
        close_callback: BoxedCallbackFn<'static>,
    ) {
        self.register_connect(rid, provider, source, close_callback);
    }

    fn get_config_dir(&self, rid: Option<PluginRid>) -> StdResult<PathBuf> {
        self.get_config_dir(rid)
    }

    fn get_data_dir(&self, rid: Option<PluginRid>) -> StdResult<PathBuf> {
        self.get_data_dir(rid)
    }
}

pub struct PluginContext<G: GlobalContext + 'static> {
    rid: PluginRid,
    global: G,
    runtime: Option<Runtime>,
}

pub type SharedPContext = Arc<PluginContext<Box<dyn GlobalContextDyn>>>;

impl<G: GlobalContext> PluginContext<G> {
    pub fn new(marker: PluginRid, global: G, runtime: Option<Runtime>) -> Self {
        Self {
            rid: marker,
            global,
            runtime,
        }
    }

    pub fn rid(&self) -> PluginRid {
        self.rid
    }

    pub fn get_shared_app(&self, rid: impl Into<AppRid>) -> Option<impl OBApp + 'static> {
        let rid = rid.into();
        self.global.get_shared_app(rid)
    }

    pub fn get_plugin_rid(&self, id: impl AsRef<str>) -> Option<PluginRid> {
        self.global.get_plugin_rid(id.as_ref())
    }

    pub fn get_config_dir(&self) -> Result<PathBuf, Box<dyn StdErr>> {
        self.global.get_config_dir(Some(self.rid))
    }

    pub fn get_data_dir(&self) -> Result<PathBuf, Box<dyn StdErr>> {
        self.global.get_data_dir(Some(self.rid))
    }

    pub fn register_connect<F, FR>(
        &self,
        provider: impl OBAppProvider + 'static,
        source: impl MessageSource + 'static,
        close_callback: F,
    ) where
        F: FnOnce() -> FR + Send + 'static,
        FR: Future<Output = StdResult<()>> + Send + 'static,
    {
        self.global
            .register_connect(self.rid, provider, source, close_callback)
    }

    pub fn at_runtime(&self) -> bool {
        self.runtime.is_some()
    }

    pub(crate) fn into_dyn(self) -> PluginContext<Box<dyn GlobalContextDyn>> {
        PluginContext {
            rid: self.rid,
            global: Box::new(self.global),
            runtime: self.runtime,
        }
    }

    /// Initializes the logger for the plugin context.
    ///
    /// This function attempts to set the logger from the runtime if it exists.
    /// If the logger is successfully set, it returns `Ok(true)`. If there is no logger
    /// available in the runtime, it returns `Ok(false)`.
    ///
    /// # Errors
    ///
    /// Returns a `log::SetLoggerError` if setting the logger fails.
    pub fn init_logger(&mut self) -> Result<bool, log::SetLoggerError> {
        if let Some(rt) = &mut self.runtime {
            let Some((logger, lvl)) = rt.logger.take() else {
                return Ok(false);
            };
            log::set_boxed_logger(logger)?;
            log::set_max_level(lvl);
        }

        Ok(true)
    }

    pub fn into_shared(self) -> SharedPContext {
        Arc::new(self.into_dyn())
    }

    pub async fn call_api<C, E>(&self, target: PluginRid, call: C) -> Result<Value, APIError>
    where
        C: IntoAPICall<Error = E>,
        E: Display,
    {
        self.global
            .call_plugin_api(
                self.rid,
                target,
                call.into_api_call().map_err(APIError::other)?,
            )
            .await
    }
}
