use std::error::Error as StdErr;
use std::fmt::Display;

mod call;
mod context;

pub use {call::*, context::*};

macro_rules! wrap {
    ($name:ident, $ty:ty $(, $doc:literal)?) => {
        $(#[doc = $doc])?
        #[derive(Debug, PartialEq, Eq, Hash, Clone, Copy, serde::Serialize, serde::Deserialize)]
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
                write!(f, concat!(stringify!($name), "({})"), self.0)
            }
        }
    };
}

wrap!(AppRid, u64, "OneBot application side's runtime id.");
wrap!(PluginRid, u64, "Plugin's runtime id.");
wrap!(Endpoint, u64);

pub struct Runtime {
    pub logger: Option<(Box<dyn log::Log>, log::LevelFilter)>,
}
