use std::{ops::Deref, sync::Arc};

use onebot_connect_interface::app::{AppDyn, OBApp};
use parking_lot::RwLock;

use crate::CarolinaPluginDyn;

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
    };
}

wrap!(AppUid, u64);
wrap!(PluginUid, u64);
wrap!(Endpoint, u64);

pub trait EventContextTrait {
    type App: OBApp + 'static;

    fn app(&self) -> &Self::App;
    fn app_marker(&self) -> AppUid;
    fn into_inner(self) -> (Self::App, AppUid);
}

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

pub trait GlobalContext: Send + Sync + 'static {
    fn get_app(&self, id: AppUid) -> Option<Arc<dyn AppDyn>>;

    fn get_plugin_uid(&self, id: &str) -> Option<PluginUid>;

    fn call_plugin_api(&self, src: PluginUid, call: APICall) -> 

    fn context_ref(&self) -> Arc<dyn GlobalContext>;
}

impl GlobalContext for Arc<dyn GlobalContext> {
    fn get_app(&self, id: AppUid) -> Option<Arc<dyn AppDyn>> {
        self.deref().get_app(id)
    }

    fn get_plugin_uid(&self, id: &str) -> Option<PluginUid> {
        self.deref().get_plugin_uid(id)
    }

    fn context_ref(&self) -> Arc<dyn GlobalContext> {
        self.clone()
    }
}

#[derive(Debug, Clone)]
pub struct APICall {
    pub target: PluginUid,
    pub endpoint: Endpoint,
    pub payload: Vec<u8>,
}

pub struct PluginContext<G: GlobalContext> {
    marker: PluginUid,
    global: G,
}

impl PluginContext {
    pub fn marker(&self) -> PluginUid {
        self.marker
    }

    pub fn get_app(&self, id: impl Into<AppUid>) -> Option<Arc<dyn AppDyn>> {
        self.global.get_app(id.into())
    }

    pub fn get_plugin_uid(&self, id: impl AsRef<str>) -> Option<PluginUid> {
        self.global.get_plugin_uid(id.as_ref())
    }

    pub fn call_api(&self) {}
}
