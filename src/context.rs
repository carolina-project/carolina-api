use std::{ops::Deref, sync::Arc};

use onebot_connect_interface::app::{AppDyn, OBApp};
use uuid::Uuid;

pub trait EventContextTrait {
    type Global: GlobalContext;
    type App: OBApp;

    fn global_context(&self) -> &Self::Global;

    fn app(&self) -> &Self::App;
}

pub struct EventContext<G: GlobalContext, A: OBApp> {
    global: G,
    app: A,
}
impl<G: GlobalContext, A: OBApp> EventContext<G, A> {
    pub fn new(global: G, app: A) -> Self {
        Self { global, app }
    }
}
impl<G: GlobalContext, A: OBApp> EventContextTrait for EventContext<G, A> {
    type App = A;
    type Global = G;

    fn global_context(&self) -> &Self::Global {
        &self.global
    }

    fn app(&self) -> &Self::App {
        &self.app
    }
}

pub struct DynEventContext {
    global: Arc<dyn GlobalContext>,
    app: Arc<dyn AppDyn>,
}
impl EventContextTrait for DynEventContext {
    type Global = Arc<dyn GlobalContext>;
    type App = Arc<dyn AppDyn>;

    fn global_context(&self) -> &Self::Global {
        &self.global
    }

    fn app(&self) -> &Self::App {
        &self.app
    }
}

pub trait GlobalContext: Send + Sync + 'static {
    fn get_app(&self, id: &Uuid) -> Option<Arc<dyn AppDyn>>;
}

impl GlobalContext for Arc<dyn GlobalContext> {
    fn get_app(&self, id: &Uuid) -> Option<Arc<dyn AppDyn>> {
        self.deref().get_app(id)
    }
}
