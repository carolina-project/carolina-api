use std::{fmt::Display, future::Future, ops::Deref, pin::Pin};

use onebot_connect_interface::app::{AppDyn, OBApp};
use std::error::Error as ErrTrait;

macro_rules! wrap {
    ($name:ident, $ty:ty) => {
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

wrap!(AppUid, u64);
wrap!(PluginUid, u64);
wrap!(Endpoint, u64);

#[derive(Debug, Clone)]
pub struct APICall {
    pub target: PluginUid,
    pub endpoint: Endpoint,
    pub payload: Vec<u8>,
}

pub type APIResult = Result<Vec<u8>, APIError>;

pub trait EventContextTrait {
    type App: OBApp + 'static;

    fn app(&self) -> &Self::App;
    fn app_marker(&self) -> AppUid;
    fn into_inner(self) -> (Self::App, AppUid);
}

/// Event context for static dispatching.
pub struct EventContext<A: OBApp + 'static> {
    marker: AppUid,
    app: A,
}
impl<A: OBApp> EventContext<A> {
    pub fn new(marker: AppUid, app: A) -> Self {
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
    fn app_marker(&self) -> AppUid {
        self.marker
    }

    #[inline]
    fn into_inner(self) -> (Self::App, AppUid) {
        (self.app, self.marker)
    }
}

/// Event context for dynamic dispatching.
pub struct DynEventContext {
    app_uid: AppUid,
    app: Box<dyn AppDyn>,
}
impl<A: OBApp + 'static> From<(A, AppUid)> for DynEventContext {
    fn from(app: (A, AppUid)) -> Self {
        Self::new(app.0, app.1)
    }
}
impl DynEventContext {
    fn new(app: impl AppDyn + 'static, app_id: AppUid) -> Self {
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

    fn app_marker(&self) -> AppUid {
        self.app_uid
    }

    fn into_inner(self) -> (Self::App, AppUid) {
        (self.app, self.app_uid)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum APIError {
    #[error("target plugin not found: {0}")]
    PluginNotFound(PluginUid),
    #[error("endpoint not found: {0}")]
    EndpointNotFound(Endpoint),
    #[error("api call error: {0}")]
    Error(#[from] Box<dyn ErrTrait>),
}

pub trait GlobalContext: Send + Sync + 'static {
    fn get_app(&self, id: AppUid) -> Option<Box<dyn AppDyn>>;

    fn get_plugin_uid(&self, id: &str) -> Option<PluginUid>;

    fn call_plugin_api(
        &self,
        src: PluginUid,
        call: APICall,
    ) -> impl Future<Output = APIResult> + Send + '_;
}

pub trait GlobalContextDyn: Send + Sync + 'static {
    fn get_app(&self, id: AppUid) -> Option<Box<dyn AppDyn>>;

    fn get_plugin_uid(&self, id: &str) -> Option<PluginUid>;

    fn call_plugin_api(
        &self,
        src: PluginUid,
        call: APICall,
    ) -> Pin<Box<dyn Future<Output = APIResult> + Send + '_>>;
}

// For parameter passing
impl GlobalContext for Box<dyn GlobalContextDyn> {
    fn get_app(&self, id: AppUid) -> Option<Box<dyn AppDyn>> {
        self.deref().get_app(id)
    }

    fn get_plugin_uid(&self, id: &str) -> Option<PluginUid> {
        self.deref().get_plugin_uid(id)
    }

    fn call_plugin_api(
        &self,
        src: PluginUid,
        call: APICall,
    ) -> impl Future<Output = APIResult> + Send + '_ {
        self.deref().call_plugin_api(src, call)
    }
}

impl<T: GlobalContext> GlobalContextDyn for T {
    fn get_app(&self, id: AppUid) -> Option<Box<dyn AppDyn>> {
        self.get_app(id).map(|a| Box::new(a) as _)
    }

    fn get_plugin_uid(&self, id: &str) -> Option<PluginUid> {
        self.get_plugin_uid(id)
    }

    fn call_plugin_api(
        &self,
        src: PluginUid,
        call: APICall,
    ) -> Pin<Box<dyn Future<Output = APIResult> + Send + '_>> {
        Box::pin(self.call_plugin_api(src, call))
    }
}

pub struct PluginContext<G: GlobalContext> {
    marker: PluginUid,
    global: G,
}
impl<G: GlobalContext> PluginContext<G> {
    pub fn marker(&self) -> PluginUid {
        self.marker
    }

    pub fn get_app(&self, id: impl Into<AppUid>) -> Option<impl OBApp + 'static> {
        let uid = id.into();
        self.global.get_app(uid)
    }

    pub fn get_plugin_uid(&self, id: impl AsRef<str>) -> Option<PluginUid> {
        self.global.get_plugin_uid(id.as_ref())
    }

    pub fn call_api(&self) {}
}
