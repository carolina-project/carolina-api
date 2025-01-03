pub mod common;

#[cfg(feature = "plugin")]
pub mod plugin;

pub use carolina_api_macros::plugin_api;
pub use common::*;
pub use onebot_connect_interface as oc_interface;
pub use onebot_connect_interface::types;
pub use types::{ob12::event::RawEvent, OBEventSelector};

pub use std::error::Error as StdErr;

pub type StdResult<T> = Result<T, Box<dyn StdErr>>;
