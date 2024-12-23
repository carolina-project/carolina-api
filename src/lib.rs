use std::{future::Future, pin::Pin, sync::Arc};

use context::{DynEventContext, EventContextTrait, GlobalContext};
use onebot_connect_interface::types::ob12::event::RawEvent;
use plugin_api::plugin_api;
use std::error::Error as ErrTrait;

pub mod context;

pub use onebot_connect_interface as interface;

pub struct EventType {
    pub r#type: String,
    pub detail_type: String,
}

#[plugin_api(
    dyn_t = CarolinaPluginDyn,
    use_p = std::future::Future,
    use_p = std::error::Error,
)]
pub trait CarolinaPlugin {
    fn init(
        &mut self,
        context: impl GlobalContext,
    ) -> impl Future<Output = Result<(), Box<dyn Error>>> + Send + '_;

    fn register_events(&mut self) -> impl Future<Output = Vec<EventType>> + Send + '_;

    fn handle_event<EC>(
        &self,
        event: RawEvent,
        context: &EC,
    ) -> impl Future<Output = Result<(), Box<dyn Error>>> + Send + '_
    where
        EC: EventContextTrait;

    fn deinit(&mut self) -> impl Future<Output = Result<(), Box<dyn Error>>> + Send + '_;
}

type PinBox<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;
type PinBoxResult<'a, T> = PinBox<'a, Result<T, Box<dyn ErrTrait>>>;

pub trait CarolinaPluginDyn {
    fn init(&mut self, context: Arc<dyn GlobalContext>) -> PinBoxResult<()>;

    fn register_events(&mut self) -> PinBox<Vec<EventType>>;

    fn handle_event(&self, event: RawEvent, context: &DynEventContext) -> PinBoxResult<()>;

    fn deinit(&mut self) -> PinBoxResult<()>;
}

impl<T: CarolinaPlugin> CarolinaPluginDyn for T {
    fn init(&mut self, context: Arc<dyn GlobalContext>) -> PinBoxResult<()> {
        Box::pin(self.init(context))
    }

    fn register_events(&mut self) -> PinBox<Vec<EventType>> {
        Box::pin(self.register_events())
    }

    fn handle_event(&self, event: RawEvent, context: &DynEventContext) -> PinBoxResult<()> {
        Box::pin(self.handle_event(event, context))
    }

    fn deinit(&mut self) -> PinBoxResult<()> {
        Box::pin(self.deinit())
    }
}
