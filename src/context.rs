use std::{ops::Deref, sync::Arc};

use onebot_connect_interface::app::{AppDyn, OBApp};
use uuid::Uuid;

pub trait EventContextTrait {
    type App: OBApp;

    fn global_context(&self) -> &dyn GlobalContext;

    fn app(&self) -> &Self::App;
}

pub struct EventContext<A: OBApp> {
    global: Arc<dyn GlobalContext>,
    app: A,
}
impl<A: OBApp> EventContext<A> {
    pub fn new(global: Arc<dyn GlobalContext>, app: A) -> Self {
        Self { global, app }
    }
}
impl<A: OBApp> EventContextTrait for EventContext<A> {
    type App = A;

    fn global_context(&self) -> &dyn GlobalContext {
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
    type App = Arc<dyn AppDyn>;

    fn global_context(&self) -> &dyn GlobalContext {
        &self.global
    }

    fn app(&self) -> &Self::App {
        &self.app
    }
}

pub trait GlobalContext: Send + Sync + 'static {
    fn get_app(&self, id: &Uuid) -> Option<Arc<dyn AppDyn>>;

    fn context_ref(&self) -> Arc<dyn GlobalContext>;
}

impl GlobalContext for Arc<dyn GlobalContext> {
    fn get_app(&self, id: &Uuid) -> Option<Arc<dyn AppDyn>> {
        self.deref().get_app(id)
    }

    fn context_ref(&self) -> Arc<dyn GlobalContext> {
        self.clone()
    }
}
