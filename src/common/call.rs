use onebot_connect_interface::value::Value;

use super::*;

#[derive(Debug, thiserror::Error)]
pub enum APIError {
    #[error("target plugin not found: {0}")]
    PluginNotFound(PluginRid),
    #[error("endpoint not found: {0}")]
    EndpointNotFound(Endpoint),
    #[error("api call error: {0}")]
    Error(String),
}

impl APIError {
    pub fn other<T: Display>(e: T) -> Self {
        Self::Error(e.to_string())
    }
}

#[derive(Debug, Clone)]
pub struct APICall {
    pub endpoint: Endpoint,
    pub payload: Value,
}

pub trait IntoAPICall {
    type Error: StdErr;

    fn into_api_call(self) -> Result<APICall, Self::Error>;
}

pub type APIResult = Result<Value, APIError>;
