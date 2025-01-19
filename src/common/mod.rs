use std::fmt::Display;
use std::{error::Error as StdErr, future::Future};

mod call;
mod context;
mod plugin;

use crate::StdResult;

pub use {call::*, context::*, plugin::*};

macro_rules! id_type {
    ($name:ident, $ty:ty $(, $doc:literal)?) => {
        $(#[doc = $doc])?
        #[derive(
            Debug, PartialEq, Eq, Ord, PartialOrd, Hash, Clone, Copy, serde::Serialize, serde::Deserialize
        )]
        pub struct $name($ty);

        impl $name {
            #[inline]
            pub const fn new(id: $ty) -> Self {
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
                write!(f, "{}", self.0)
            }
        }
    };
}

id_type!(AppRid, u64, "OneBot application side's runtime id.");
id_type!(PluginRid, u64, "Plugin's runtime id.");
id_type!(Endpoint, u64, "Plugin api call endpoint id.");

pub struct Runtime {
    pub logger: Option<(Box<dyn log::Log>, log::LevelFilter)>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Priority {
    Lowest,
    Low,
    #[default]
    Medium,
    High,
    Highest,
}

impl Priority {
    pub const fn sorted() -> &'static [Priority] {
        &[
            Priority::Highest,
            Priority::High,
            Priority::Medium,
            Priority::Low,
            Priority::Lowest,
        ]
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EventState {
    #[default]
    Pass,
    Intercept,
}

pub fn pass<E>() -> Result<EventState, E> {
    Ok(EventState::Pass)
}

pub fn intercept<E>() -> Result<EventState, E> {
    Ok(EventState::Intercept)
}

#[derive(Debug, Clone)]
pub struct Subscribe {
    pub event_type: String,
    pub detail_type: Option<String>,
    pub priority: Priority,
}

impl Subscribe {
    pub fn new(event_type: impl Into<String>, detail_type: Option<impl Into<String>>) -> Self {
        Self {
            event_type: event_type.into(),
            detail_type: detail_type.map(|r| r.into()),
            priority: Priority::default(),
        }
    }

    pub fn priority(mut self, priority: Priority) -> Self {
        self.priority = priority;
        self
    }
}

pub type BoxedCallbackFn<'a, R = StdResult<()>> = Box<dyn FnOnce() -> PinBoxFut<'a, R> + Send + 'a>;

fn boxed_async_cb<'a, F, R, FR>(f: F) -> BoxedCallbackFn<'a, R>
where
    F: FnOnce() -> FR + Send + 'a,
    FR: Future<Output = R> + Send + 'a,
{
    Box::new(move || Box::pin(f()))
}

#[derive(Debug)]
pub struct ErrorDisplay(String);

impl ErrorDisplay {
    pub fn boxed_send<T: Display>(msg: T) -> Box<dyn StdErr + Send> {
        Box::new(Self(msg.to_string()))
    }
}

impl Display for ErrorDisplay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl StdErr for ErrorDisplay {}
