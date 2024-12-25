use std::{fmt::Display, future::Future, ops::Deref, path::PathBuf, pin::Pin};

use onebot_connect_interface::app::{
    AppDyn, AppProviderDyn, MessageSource, MessageSourceDyn, OBApp, OBAppProvider,
};

use crate::BResult;

macro_rules! wrap {
    ($name:ident, $ty:ty $(, $doc:literal)?) => {
        $(#[doc = $doc])?
        #[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
        pub struct $name($ty);

        impl $name {
            #[inline]
            pub fn new(id: $ty) -> Self {
                Self(id)
            }

            #[inline]
            pub fn inner(&self) -> $ty {
                self.0
            }
        }
        impl From<$name> for $ty {
            #[inline]
            fn from(val: $name) -> Self {
                val.0
            }
        }
        impl From<$ty> for $name {
            #[inline]
            fn from(id: $ty) -> Self {
                Self(id)
            }
        }
        impl Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, concat!(stringify!($name), "({})"), self.0)
            }
        }
    };
}

wrap!(AppRid, u64, "OneBot application side's runtime id.");
wrap!(PluginRid, u64, "Plugin's runtime id.");
wrap!(Endpoint, u64);

#[derive(Debug, Clone)]
pub struct APICall {
    pub target: PluginRid,
    pub endpoint: Endpoint,
    pub payload: Vec<u8>,
}

pub type APIResult = Result<Vec<u8>, APIError>;

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

    #[inline]
    fn app(&self) -> &Self::App {
        &self.app
    }

    #[inline]
    fn app_marker(&self) -> AppRid {
        self.marker
    }

    #[inline]
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

#[derive(Debug, thiserror::Error)]
pub enum APIError {
    #[error("target plugin not found: {0}")]
    PluginNotFound(PluginRid),
    #[error("endpoint not found: {0}")]
    EndpointNotFound(Endpoint),
    #[error("api call error: {0}")]
    Error(String),
}

impl APIError {
    pub fn other<T: Display>(e: T) -> Self {
        Self::Error(e.to_string())
    }
}

pub trait GlobalContext: Send + Sync + 'static {
    fn get_shared_app(&self, id: AppRid) -> Option<Box<dyn AppDyn>>;

    fn get_plugin_rid(&self, id: &str) -> Option<PluginRid>;

    fn get_plugin_id(&self, rid: impl Into<PluginRid>) -> Option<String>;

    fn call_plugin_api(
        &self,
        src: PluginRid,
        call: APICall,
    ) -> impl Future<Output = APIResult> + Send + '_;

    fn register_connect(
        &self,
        rid: PluginRid,
        provider: impl OBAppProvider,
        source: impl MessageSource,
    );

    fn get_config_dir(&self, rid: Option<PluginRid>) -> BResult<PathBuf>;

    fn get_data_dir(&self, rid: Option<PluginRid>) -> BResult<PathBuf>;
}

pub trait GlobalContextDyn: Send + Sync + 'static {
    fn get_shared_app(&self, id: AppRid) -> Option<Box<dyn AppDyn>>;

    fn get_plugin_rid(&self, id: &str) -> Option<PluginRid>;

    fn get_plugin_id(&self, rid: PluginRid) -> Option<String>;

    fn call_plugin_api(
        &self,
        src: PluginRid,
        call: APICall,
    ) -> Pin<Box<dyn Future<Output = APIResult> + Send + '_>>;

    fn register_connect(
        &self,
        uid: PluginRid,
        provider: Box<dyn AppProviderDyn>,
        source: Box<dyn MessageSourceDyn>,
    );

    fn get_config_dir(&self, rid: Option<PluginRid>) -> BResult<PathBuf>;

    fn get_data_dir(&self, rid: Option<PluginRid>) -> BResult<PathBuf>;
}

// For parameter passing
impl GlobalContext for Box<dyn GlobalContextDyn> {
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
        call: APICall,
    ) -> impl Future<Output = APIResult> + Send + '_ {
        self.deref().call_plugin_api(src, call)
    }

    fn register_connect(
        &self,
        rid: PluginRid,
        provider: impl OBAppProvider,
        source: impl MessageSource,
    ) {
        self.deref()
            .register_connect(rid, Box::new(provider), Box::new(source))
    }

    fn get_config_dir(&self, rid: Option<PluginRid>) -> BResult<PathBuf> {
        self.deref().get_config_dir(rid)
    }

    fn get_data_dir(&self, rid: Option<PluginRid>) -> BResult<PathBuf> {
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
        call: APICall,
    ) -> Pin<Box<dyn Future<Output = APIResult> + Send + '_>> {
        Box::pin(self.call_plugin_api(src, call))
    }

    fn register_connect(
        &self,
        uid: PluginRid,
        provider: Box<dyn AppProviderDyn>,
        source: Box<dyn MessageSourceDyn>,
    ) {
        self.register_connect(uid, provider, source)
    }

    fn get_config_dir(&self, rid: Option<PluginRid>) -> BResult<PathBuf> {
        self.get_config_dir(rid)
    }

    fn get_data_dir(&self, rid: Option<PluginRid>) -> BResult<PathBuf> {
        self.get_data_dir(rid)
    }
}

pub struct PluginContext<G: GlobalContext> {
    marker: PluginRid,
    global: G,
}
impl<G: GlobalContext> PluginContext<G> {
    pub fn marker(&self) -> PluginRid {
        self.marker
    }

    pub fn get_shared_app(&self, rid: impl Into<AppRid>) -> Option<impl OBApp + 'static> {
        let rid = rid.into();
        self.global.get_shared_app(rid)
    }

    pub fn get_plugin_rid(&self, id: impl AsRef<str>) -> Option<PluginRid> {
        self.global.get_plugin_rid(id.as_ref())
    }

    pub(crate) fn into_dyn(self) -> PluginContext<Box<dyn GlobalContextDyn>> {
        PluginContext {
            marker: self.marker,
            global: Box::new(self.global),
        }
    }

    pub async fn call_api<C, E>(&self, call: C) -> Result<Vec<u8>, APIError>
    where
        C: TryInto<APICall, Error = E>,
        E: Display,
    {
        self.global
            .call_plugin_api(self.marker, call.try_into().map_err(APIError::other)?)
            .await
    }
}
