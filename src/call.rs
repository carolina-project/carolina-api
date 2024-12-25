use std::{future::Future, pin::Pin};

use fxhash::FxHashMap;
use serde::Serialize;

use crate::context::*;

type CallFut<'a> = Pin<Box<dyn Future<Output = APIResult> + Send + 'a>>;
type BoxedCallFn = Box<dyn Fn(PluginRid, Vec<u8>) -> CallFut<'static>>;

pub trait APICallHandler {
    fn endpoint(&self) -> Endpoint;

    fn handle(&self, src: PluginRid, payload: Vec<u8>) -> CallFut;
}

pub struct FnCall {
    endpoint: Endpoint,
    func: BoxedCallFn,
}

impl FnCall {
    pub fn new<F, R>(endpoint: Endpoint, func: F) -> Self
    where
        F: (Fn(PluginRid, Vec<u8>) -> R) + Send + 'static,
        R: Future<Output = APIResult> + Send + 'static,
    {
        let func: BoxedCallFn = Box::new(move |plugin, payload| {
            let fut = func(plugin, payload);
            Box::pin(fut)
        });
        FnCall { endpoint, func }
    }
}

impl APICallHandler for FnCall {
    fn endpoint(&self) -> Endpoint {
        self.endpoint
    }

    fn handle(&self, src: PluginRid, payload: Vec<u8>) -> CallFut {
        (self.func)(src, payload)
    }
}

#[cfg(feature = "bincode")]
mod deser_handler {
    use serde::de::DeserializeOwned;

    use super::*;

    pub struct BincodeHandler {
        endpoint: Endpoint,
        func: BoxedCallFn,
    }

    impl BincodeHandler {
        pub fn new<F, R, SR, T>(endpoint: Endpoint, func: F) -> Self
        where
            T: DeserializeOwned,
            F: (Fn(PluginRid, T) -> R) + Send + 'static,
            SR: Serialize,
            R: Future<Output = Result<SR, APIError>> + Send + 'static,
        {
            let func: BoxedCallFn =
                Box::new(
                    move |plugin, payload| match bincode::deserialize(&payload) {
                        Ok(res) => {
                            let fut = func(plugin, res);
                            Box::pin(async move {
                                fut.await
                                    .and_then(|r| bincode::serialize(&r).map_err(APIError::other))
                            })
                        }
                        Err(e) => Box::pin(async move {
                            Err(APIError::Error(format!("deserialize error: {e}")))
                        }),
                    },
                );
            BincodeHandler { endpoint, func }
        }
    }

    impl APICallHandler for BincodeHandler {
        fn endpoint(&self) -> Endpoint {
            self.endpoint
        }

        fn handle(&self, src: PluginRid, payload: Vec<u8>) -> CallFut {
            (self.func)(src, payload)
        }
    }
}

#[cfg(feature = "bincode")]
pub use deser_handler::*;

#[derive(Default)]
pub struct APISet {
    handlers: FxHashMap<Endpoint, Box<dyn APICallHandler>>,
}

impl APISet {
    pub fn register(&mut self, handler: impl APICallHandler + 'static) {
        self.handlers.insert(handler.endpoint(), Box::new(handler));
    }

    pub async fn handle(&self, src: PluginRid, call: APICall) -> Result<Vec<u8>, APIError> {
        let APICall {
            endpoint, payload, ..
        } = call;

        if let Some(handler) = self.handlers.get(&endpoint) {
            let result = handler.handle(src, payload).await?;
            Ok(result)
        } else {
            Err(APIError::EndpointNotFound(endpoint))
        }
    }

    pub fn is_registered(&self, endpoint: Endpoint) -> bool {
        self.handlers.contains_key(&endpoint)
    }
}