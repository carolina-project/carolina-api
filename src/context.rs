use std::{ops::Deref, sync::Arc};

use onebot_connect_interface::app::{AppDyn, OBApp};

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub struct AppMarker(u64);

impl AppMarker {
    #[inline]
    pub fn new(id: u64) -> Self {
        Self(id)
    }

    #[inline]
    pub fn into_inner(self) -> u64 {
        self.0
    }
}
impl Into<u64> for AppMarker {
    #[inline]
    fn into(self) -> u64 {
        self.0
    }
}
impl From<u64> for AppMarker {
    #[inline]
    fn from(id: u64) -> Self {
        Self(id)
    }
}

pub trait EventContextTrait {
    type App: OBApp + 'static;

    fn app(&self) -> &Self::App;
    fn app_marker(&self) -> AppMarker;
    fn into_inner(self) -> (Self::App, AppMarker);
}

pub struct EventContext<A: OBApp + 'static> {
    marker: AppMarker,
    app: A,
}

impl<A: OBApp> EventContext<A> {
    pub fn new(marker: AppMarker, app: A) -> Self {
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
    fn app_marker(&self) -> AppMarker {
        self.marker
    }

    #[inline]
    fn into_inner(self) -> (Self::App, AppMarker) {
        (self.app, self.marker)
    }
}

pub struct DynEventContext {
    marker: AppMarker,
    app: Box<dyn AppDyn>,
}
impl<A: OBApp + 'static> From<(A, AppMarker)> for DynEventContext {
    fn from(app: (A, AppMarker)) -> Self {
        Self::new(app.0, app.1)
    }
}
impl DynEventContext {
    fn new(app: impl AppDyn + 'static, marker: AppMarker) -> Self {
        Self {
            marker,
            app: Box::new(app),
        }
    }
}

impl EventContextTrait for DynEventContext {
    type App = Box<dyn AppDyn>;

    fn app(&self) -> &Self::App {
        &self.app
    }

    fn app_marker(&self) -> AppMarker {
        self.marker
    }

    fn into_inner(self) -> (Self::App, AppMarker) {
        (self.app, self.marker)
    }
}

pub trait GlobalContext: Send + Sync + 'static {
    fn get_app(&self, id: AppMarker) -> Option<Arc<dyn AppDyn>>;

    fn context_ref(&self) -> Arc<dyn GlobalContext>;
}

impl GlobalContext for Arc<dyn GlobalContext> {
    fn get_app(&self, id: AppMarker) -> Option<Arc<dyn AppDyn>> {
        self.deref().get_app(id)
    }

    fn context_ref(&self) -> Arc<dyn GlobalContext> {
        self.clone()
    }
}
