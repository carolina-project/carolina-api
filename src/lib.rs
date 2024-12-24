use std::{future::Future, pin::Pin, sync::Arc};

use context::{DynEventContext, GlobalContext};
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

type BResult<T> = Result<T, Box<dyn ErrTrait + Send>>;

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
            event: E::deserialize_event(event).map_err(|e| Box::new(e) as _)?,
        })
    }
}

#[plugin_api(
    ignore(handle_event_selected),
    dyn_t = CarolinaPluginDyn,
)]
mod plugin {
    use crate::context::{EventContextTrait, GlobalContext};
    use crate::EventSelected;
    use onebot_connect_interface::types::{ob12::event::RawEvent, OBEventSelector};
    use std::error::Error as ErrTrait;
    use std::{error::Error, future::Future};
    type BResult<T> = Result<T, Box<dyn ErrTrait + Send>>;

    pub trait CarolinaPlugin: Sync {
        type Event: OBEventSelector + Send;

        fn init(
            &mut self,
            context: &dyn GlobalContext,
        ) -> impl Future<Output = BResult<()>> + Send + '_;

        fn register_events(&self) -> impl Future<Output = Vec<(String, String)>> {
            async {
                Self::Event::get_selectable()
                    .iter()
                    .map(|desc| (desc.r#type.to_owned(), desc.detail_type.to_owned()))
                    .collect()
            }
        }

        fn handle_event<'a, 'b: 'a, EC>(
            &'a self,
            event: RawEvent,
            context: &'b EC,
        ) -> impl Future<Output = BResult<()>> + Send + '_
        where
            EC: EventContextTrait + Send + Sync,
        {
            async move {
                self.handle_event_selected(EventSelected::parse(event)?, context)
                    .await
            }
        }

        #[allow(unused)]
        fn handle_event_selected<EC>(
            &self,
            event: EventSelected<Self::Event>,
            context: &EC,
        ) -> impl Future<Output = BResult<()>> + Send + '_
        where
            EC: EventContextTrait,
        {
            async { Ok(()) }
        }

        fn deinit(&mut self) -> impl Future<Output = Result<(), Box<dyn Error>>> + Send + '_;
    }
}

type PinBox<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;
type PinBoxResult<'a, T> = PinBox<'a, Result<T, Box<dyn ErrTrait + Send>>>;

pub trait CarolinaPluginDyn {
    fn init(&mut self, context: &dyn GlobalContext) -> PinBoxResult<()>;

    fn register_events(&mut self) -> PinBox<Vec<(String, String)>>;

    fn handle_event(&self, event: RawEvent, context: &DynEventContext) -> PinBoxResult<()>;

    fn deinit(&mut self) -> PinBoxResult<()>;
}

struct Placeholder;

impl OBEventSelector for Placeholder {
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

impl CarolinaPlugin for dyn CarolinaPluginDyn {
    type Event = Placeholder;

    fn init<G: GlobalContext>(
        &mut self,
        context: &G,
    ) -> impl Future<Output = BResult<()>> + Send + '_ {
        self.init(context)
    }

    fn deinit(&mut self) -> impl Future<Output = Result<(), Box<dyn Error>>> + Send + '_ {
        todo!()
    }
}
