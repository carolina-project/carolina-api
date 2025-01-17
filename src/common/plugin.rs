use std::{
    future::Future,
    ops::{Deref, DerefMut},
    pin::Pin,
    sync::Arc,
};

use crate::*;

#[derive(Debug, Clone)]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
}

#[derive(Debug)]
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
            version: self.version.unwrap_or_else(|| "0.1.0".to_string()),
            author: self.author.unwrap_or_else(|| "Unknown".to_string()),
            description: self
                .description
                .unwrap_or_else(|| "No description provided.".to_string()),
        }
    }
}

#[macro_export]
/// Macro to create a `PluginInfo` instance with metadata from the Cargo package.
///
/// This macro can be used to generate a `PluginInfo` struct with metadata such as
/// the package name, version, authors, and description. It can be invoked with
/// or without a custom name for the plugin.
///
/// # Examples
///
/// ```
/// use carolina_api::plugin_info;
///
/// // Using the macro without a custom name
/// let info = plugin_info!();
///
/// // Using the macro with a custom name
/// let info = plugin_info!("Custom Plugin Name");
/// ```
///
/// The generated `PluginInfo` will have the following fields populated from the
/// Cargo package metadata:
/// - `id`: The package name
/// - `name`: The custom name provided or the package name if not provided
/// - `version`: The package version
/// - `author`: The package authors
/// - `description`: The package description
macro_rules! plugin_info {
    ($name:literal) => {
        $crate::PluginInfo {
            id: env!("CARGO_PKG_NAME").to_string(),
            name: $name.into(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            author: env!("CARGO_PKG_AUTHORS").to_string(),
            description: env!("CARGO_PKG_DESCRIPTION").to_string(),
        }
    };
    () => {
        $crate::PluginInfo {
            id: env!("CARGO_PKG_NAME").to_string(),
            name: env!("CARGO_PKG_NAME").to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            author: env!("CARGO_PKG_AUTHORS").to_string(),
            description: env!("CARGO_PKG_DESCRIPTION").to_string(),
        }
    };
}

pub trait SelectorExt {
    fn subscribe() -> Vec<Subscribe>;
}
impl<T: OBEventSelector> SelectorExt for T {
    fn subscribe() -> Vec<Subscribe> {
        Self::get_selectable()
            .iter()
            .map(|desc| Subscribe::new(desc.r#type.to_owned(), Some(desc.detail_type.to_owned())))
            .collect()
    }
}

#[plugin_api(
    dyn_t = CarolinaPluginDyn,
    dyn_wrap = plugin::DynPlugin,
)]
mod caro_plugin {
    use crate::PluginInfo;
    use crate::{APICall, APIError, APIResult, PluginContext, PluginRid};
    use crate::{EventContextTrait, GlobalContext};
    use std::future;
    use std::future::Future;

    use crate::{pass, SharedEvent};
    use crate::{EventState, Subscribe};

    pub trait CarolinaPlugin: Send + Sync {
        fn info(&self) -> PluginInfo;

        #[allow(unused)]
        fn init<G: GlobalContext>(
            &mut self,
            context: PluginContext<G>,
        ) -> impl Future<Output = Result<(), Box<dyn std::error::Error>>> + Send + '_ {
            async { Ok(()) }
        }

        #[allow(unused)]
        fn post_init<G: GlobalContext>(
            &mut self,
            context: PluginContext<G>,
        ) -> impl Future<Output = Result<(), Box<dyn std::error::Error>>> + Send + '_ {
            async { Ok(()) }
        }

        fn subscribe_events(&mut self) -> impl Future<Output = Vec<Subscribe>> + Send + '_ {
            future::ready(vec![])
        }

        #[allow(unused)]
        fn handle_event<EC>(
            &self,
            event: SharedEvent,
            context: EC,
        ) -> impl Future<Output = Result<EventState, Box<dyn std::error::Error>>> + Send + '_
        where
            EC: EventContextTrait + Send + 'static,
        {
            async { pass() }
        }

        #[allow(unused)]
        fn handle_api_call(
            &self,
            src: PluginRid,
            call: APICall,
        ) -> impl Future<Output = APIResult> + Send + '_ {
            future::ready(Err(APIError::EndpointNotFound(call.endpoint)))
        }

        fn deinit(self) -> impl Future<Output = Result<(), Box<dyn std::error::Error>>> + Send
        where
            Self: Sized,
        {
            async { Ok(()) }
        }
    }
}

pub type PinBoxFut<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;
pub type PinBoxResult<'a, T> = PinBoxFut<'a, Result<T, Box<dyn StdErr>>>;
pub type PinBoxAPIResult<'a> = PinBoxFut<'a, APIResult>;
pub type SharedEvent = Arc<RawEvent>;

pub trait CarolinaPluginDyn: Send + Sync + 'static {
    fn info(&self) -> PluginInfo;

    fn init(&mut self, context: PluginContext<Box<dyn GlobalContextDyn>>) -> PinBoxResult<()>;

    fn post_init(&mut self, context: PluginContext<Box<dyn GlobalContextDyn>>) -> PinBoxResult<()>;

    fn subscribe_events(&mut self) -> PinBoxFut<Vec<Subscribe>>;

    fn handle_event(
        &self,
        event: SharedEvent,
        context: DynEventContext,
    ) -> PinBoxResult<EventState>;

    fn handle_api_call(&self, src: PluginRid, call: APICall) -> PinBoxAPIResult;

    fn deinit(self) -> PinBoxResult<'static, ()>;
}

impl<T: CarolinaPlugin + 'static> CarolinaPluginDyn for T {
    fn info(&self) -> PluginInfo {
        self.info()
    }

    fn init(&mut self, context: PluginContext<Box<dyn GlobalContextDyn>>) -> PinBoxResult<()> {
        Box::pin(self.init(context))
    }

    fn post_init(&mut self, context: PluginContext<Box<dyn GlobalContextDyn>>) -> PinBoxResult<()> {
        Box::pin(self.post_init(context))
    }

    fn subscribe_events(&mut self) -> PinBoxFut<Vec<Subscribe>> {
        Box::pin(self.subscribe_events())
    }

    fn handle_event(
        &self,
        event: SharedEvent,
        context: DynEventContext,
    ) -> PinBoxResult<EventState> {
        Box::pin(self.handle_event(event, context))
    }

    fn handle_api_call(&self, src: PluginRid, call: APICall) -> PinBoxAPIResult {
        Box::pin(self.handle_api_call(src, call))
    }

    fn deinit(self) -> PinBoxResult<'static, ()> {
        Box::pin(self.deinit())
    }
}

// For dynamic dispatching
impl<'a> CarolinaPlugin for Box<dyn CarolinaPluginDyn + 'a> {
    fn info(&self) -> PluginInfo {
        self.deref().info()
    }

    fn init<G: GlobalContext>(
        &mut self,
        context: PluginContext<G>,
    ) -> impl Future<Output = StdResult<()>> + Send + '_ {
        self.deref_mut().init(context.into_dyn())
    }

    fn post_init<G: GlobalContext>(
        &mut self,
        context: PluginContext<G>,
    ) -> impl Future<Output = Result<(), Box<dyn StdErr>>> + Send + '_ {
        self.deref_mut().post_init(context.into_dyn())
    }

    fn subscribe_events(&mut self) -> impl Future<Output = Vec<Subscribe>> + Send + '_ {
        self.deref_mut().subscribe_events()
    }

    fn handle_api_call(
        &self,
        src: PluginRid,
        call: APICall,
    ) -> impl Future<Output = APIResult> + Send + '_ {
        self.deref().handle_api_call(src, call)
    }

    fn deinit(self) -> impl Future<Output = StdResult<()>> + Send {
        CarolinaPluginDyn::deinit(self)
    }

    fn handle_event<EC>(
        &self,
        event: SharedEvent,
        context: EC,
    ) -> impl Future<Output = StdResult<EventState>> + Send + '_
    where
        EC: EventContextTrait + Send + 'static,
    {
        self.deref()
            .handle_event(event, DynEventContext::from(context.into_inner()))
    }
}
