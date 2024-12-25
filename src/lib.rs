use std::{future::Future, pin::Pin};

use context::{
    APICall, APIResult, DynEventContext, EventContextTrait, GlobalContext, GlobalContextDyn,
    PluginContext, PluginRid,
};
use onebot_connect_interface::{
    types::ob12::event::EventDetail,
    value::{DeserializerError, SerializerError},
};
use plugin_api::plugin_api;
use serde::{Deserialize, Serialize};
use std::error::Error as ErrTrait;

pub mod context;

#[cfg(feature = "plugin")]
pub mod call;
#[cfg(feature = "framework")]
pub mod framework;

pub use onebot_connect_interface as oc_interface;
pub use onebot_connect_interface::types;

pub use types::{ob12::event::RawEvent, OBEventSelector};

type BResult<T> = Result<T, Box<dyn ErrTrait>>;

#[cfg(feature = "plugin")]
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct EventSelected<E>
where
    E: OBEventSelector,
{
    pub id: String,
    pub time: f64,
    pub event: E,
}

pub struct PluginInfoBuilder {
    id: String,
    name: Option<String>,
    version: Option<String>,
    author: Option<String>,
    description: Option<String>,
}

impl PluginInfoBuilder {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: None,
            version: None,
            author: None,
            description: None,
        }
    }

    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    pub fn author(mut self, author: impl Into<String>) -> Self {
        self.author = Some(author.into());
        self
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn build(self) -> PluginInfo {
        PluginInfo {
            name: self.name.unwrap_or_else(|| self.id.clone()),
            id: self.id,
            version: self.version.unwrap_or_else(|| "0.1.0".into()),
            author: self.author.unwrap_or_else(|| "anonymous".into()),
            description: self.description,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: Option<String>,
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
    use crate::context::{APICall, APIError, APIResult, PluginContext, PluginRid};
    use crate::context::{EventContextTrait, GlobalContext};
    use crate::{EventSelected, PluginInfo};
    use onebot_connect_interface::types::{ob12::event::RawEvent, OBEventSelector};
    use std::error::Error as ErrTrait;
    use std::future;
    use std::{error::Error, future::Future};
    type BResult<T> = Result<T, Box<dyn ErrTrait>>;

    pub trait CarolinaPlugin: Send + Sync + 'static {
        type Event: OBEventSelector + Send;

        fn info(&self) -> PluginInfo;

        fn init<G: GlobalContext>(
            &mut self,
            context: PluginContext<G>,
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
            src: PluginRid,
            call: APICall,
        ) -> impl Future<Output = APIResult> + Send + '_ {
            future::ready(Err(APIError::EndpointNotFound(call.endpoint)))
        }

        fn deinit(&mut self) -> impl Future<Output = Result<(), Box<dyn Error>>> + Send + '_;

        #[allow(unused)]
        #[cfg(feature = "plugin")]
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
type PinBoxAPIResult<'a> = PinBox<'a, APIResult>;

pub trait CarolinaPluginDyn: Send + Sync + 'static {
    fn info(&self) -> PluginInfo;

    fn init(&mut self, context: PluginContext<Box<dyn GlobalContextDyn>>) -> PinBoxResult<()>;

    fn register_events(&self) -> PinBox<Vec<(String, String)>>;

    fn handle_event(&self, event: RawEvent, context: DynEventContext) -> PinBoxResult<()>;

    fn handle_api_call(&self, src: PluginRid, call: APICall) -> PinBoxAPIResult;

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
    fn info(&self) -> PluginInfo {
        self.info()
    }

    fn init(&mut self, context: PluginContext<Box<dyn GlobalContextDyn>>) -> PinBoxResult<()> {
        Box::pin(self.init(context))
    }

    fn register_events(&self) -> PinBox<Vec<(String, String)>> {
        Box::pin(self.register_events())
    }

    fn handle_event(&self, event: RawEvent, context: DynEventContext) -> PinBoxResult<()> {
        Box::pin(self.handle_event(event, context))
    }

    fn handle_api_call(&self, src: PluginRid, call: APICall) -> PinBoxAPIResult {
        Box::pin(self.handle_api_call(src, call))
    }

    fn deinit(&mut self) -> PinBoxResult<()> {
        Box::pin(self.deinit())
    }
}

// For dynamic dispatching
impl CarolinaPlugin for dyn CarolinaPluginDyn {
    type Event = _Placeholder;

    fn info(&self) -> PluginInfo {
        self.info()
    }

    fn init<G: GlobalContext>(
        &mut self,
        context: PluginContext<G>,
    ) -> impl Future<Output = BResult<()>> + Send + '_ {
        self.init(context.into_dyn())
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
