use std::{future::Future, ops::Deref, pin::Pin, sync::Arc};

use context::{APICall, DynEventContext, Endpoint, EventContextTrait, GlobalContext};
use onebot_connect_interface::{
    types::{
        ob12::event::{EventDetail, RawEvent},
        OBEventSelector,
    },
    value::{DeserializerError, SerializerError},
};
use plugin_api::plugin_api;
use serde::{Deserialize, Serialize};
use std::error::Error as ErrTrait;

pub mod context;

type BResult<T> = Result<T, Box<dyn ErrTrait>>;

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct EventSelected<E>
where
    E: OBEventSelector,
{
    pub id: String,
    pub time: f64,
    pub event: E,
}

impl<E: OBEventSelector> EventSelected<E> {
    pub fn parse(event: RawEvent) -> BResult<Self> {
        let RawEvent { id, time, event } = event;
        Ok(Self {
            id,
            time,
            event: E::deserialize_event(event)?,
        })
    }
}

#[plugin_api(
    ignore(handle_event_selected),
    dyn_t = CarolinaPluginDyn,
)]
mod plugin {
    use crate::context::{Endpoint, EventContextTrait, GlobalContext};
    use crate::EventSelected;
    use onebot_connect_interface::types::{ob12::event::RawEvent, OBEventSelector};
    use std::error::Error as ErrTrait;
    use std::sync::Arc;
    use std::{error::Error, future::Future};
    type BResult<T> = Result<T, Box<dyn ErrTrait>>;

    pub trait CarolinaPlugin {
        type Event: OBEventSelector + Send;

        fn init(
            &mut self,
            context: Arc<dyn GlobalContext>,
        ) -> impl Future<Output = BResult<()>> + Send + '_;

        fn register_events(&self) -> impl Future<Output = Vec<(String, String)>> + Send + '_ {
            async {
                Self::Event::get_selectable()
                    .iter()
                    .map(|desc| (desc.r#type.to_owned(), desc.detail_type.to_owned()))
                    .collect()
            }
        }

        fn handle_event<EC>(
            &self,
            event: RawEvent,
            context: EC,
        ) -> impl Future<Output = BResult<()>> + Send + '_
        where
            EC: EventContextTrait + Send + 'static,
        {
            let res = EventSelected::parse(event)
                .map(|r| self.handle_event_selected(r, context))
                .map_err(|e| e.to_string());
            Box::pin(async move { res?.await })
        }

        #[allow(unused)]
        fn handle_api_call(
            &self,
            endpoint: Endpoint,
            payload: Vec<u8>,
        ) -> impl Future<Output = BResult<Vec<u8>>> + Send + '_ {
            async { Err("api call not supported".into()) }
        }

        fn deinit(&mut self) -> impl Future<Output = Result<(), Box<dyn Error>>> + Send + '_;

        #[allow(unused)]
        fn handle_event_selected<EC>(
            &self,
            event: EventSelected<Self::Event>,
            context: EC,
        ) -> impl Future<Output = BResult<()>> + Send + '_
        where
            EC: EventContextTrait,
        {
            async { Ok(()) }
        }
    }
}

type PinBox<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;
type PinBoxResult<'a, T> = PinBox<'a, Result<T, Box<dyn ErrTrait>>>;

pub trait CarolinaPluginDyn {
    fn init(&mut self, context: Arc<dyn GlobalContext>) -> PinBoxResult<()>;

    fn register_events(&self) -> PinBox<Vec<(String, String)>>;

    fn handle_event(&self, event: RawEvent, context: DynEventContext) -> PinBoxResult<()>;

    fn handle_api_call(&self, endpoint: Endpoint, payload: Vec<u8>) -> PinBoxResult<Vec<u8>>;

    fn deinit(&mut self) -> PinBoxResult<()>;
}

#[doc(hidden)]
pub struct _Placeholder;

impl OBEventSelector for _Placeholder {
    fn deserialize_event(_: EventDetail) -> Result<Self, DeserializerError>
    where
        Self: Sized,
    {
        Err(serde::de::Error::custom("not supported"))
    }

    fn serialize_event(&self) -> Result<EventDetail, SerializerError> {
        Err(serde::ser::Error::custom("not supported"))
    }

    fn get_selectable() -> &'static [onebot_connect_interface::types::base::EventDesc] {
        &[]
    }
}

impl<T: CarolinaPlugin> CarolinaPluginDyn for T {
    fn init(&mut self, context: Arc<dyn GlobalContext>) -> PinBoxResult<()> {
        Box::pin(self.init(context))
    }

    fn register_events(&self) -> PinBox<Vec<(String, String)>> {
        Box::pin(self.register_events())
    }

    fn handle_event(&self, event: RawEvent, context: DynEventContext) -> PinBoxResult<()> {
        Box::pin(self.handle_event(event, context))
    }

    fn handle_api_call(&self, endpoint: Endpoint, payload: Vec<u8>) -> PinBoxResult<Vec<u8>> {
        Box::pin(self.handle_api_call(endpoint, payload))
    }

    fn deinit(&mut self) -> PinBoxResult<()> {
        Box::pin(self.deinit())
    }
}

impl CarolinaPlugin for dyn CarolinaPluginDyn {
    type Event = _Placeholder;

    fn init(
        &mut self,
        context: Arc<dyn GlobalContext>,
    ) -> impl Future<Output = BResult<()>> + Send + '_ {
        self.init(context)
    }

    fn register_events(&self) -> impl Future<Output = Vec<(String, String)>> {
        self.register_events()
    }

    fn deinit(&mut self) -> impl Future<Output = BResult<()>> + Send + '_ {
        self.deinit()
    }

    fn handle_event<EC>(
        &self,
        event: RawEvent,
        context: EC,
    ) -> impl Future<Output = BResult<()>> + Send + '_
    where
        EC: EventContextTrait + Send + 'static,
    {
        self.handle_event(event, DynEventContext::from(context.into_inner()))
    }
}
