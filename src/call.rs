use std::{future::Future, pin::Pin, sync::Arc};

use fxhash::FxHashMap;
use serde::Serialize;

use crate::{context::*, PinBoxFut};

type CallFut<'a> = Pin<Box<dyn Future<Output = APIResult> + Send + 'a>>;

pub trait APICallHandler: Send + Sync {
    fn endpoint(&self) -> Endpoint;

    fn handle(&self, src: PluginRid, payload: Vec<u8>) -> CallFut;
}

pub trait HandlerTrait<I, R>: Send + Sync {
    fn handle(&self, src: PluginRid, input: I) -> PinBoxFut<Result<R, APIError>>;
}

impl<I, R, F, FR> HandlerTrait<I, R> for F
where
    F: Fn(PluginRid, I) -> FR + Send + Sync + 'static,
    FR: Future<Output = Result<R, APIError>> + Send + 'static,
{
    fn handle(&self, src: PluginRid, input: I) -> PinBoxFut<Result<R, APIError>> {
        Box::pin((self)(src, input))
    }
}

pub struct FnHandler {
    endpoint: Endpoint,
    handler: Box<dyn HandlerTrait<Vec<u8>, Vec<u8>>>,
}

impl FnHandler {
    pub fn new<H>(endpoint: impl Into<Endpoint>, handler: H) -> Self
    where
        H: HandlerTrait<Vec<u8>, Vec<u8>> + 'static,
    {
        FnHandler {
            endpoint: endpoint.into(),
            handler: Box::new(handler),
        }
    }
}

impl APICallHandler for FnHandler {
    fn endpoint(&self) -> Endpoint {
        self.endpoint
    }

    fn handle(&self, src: PluginRid, payload: Vec<u8>) -> CallFut {
        self.handler.handle(src, payload)
    }
}

#[cfg(feature = "bincode")]
mod deser_handler {
    use std::future;

    use super::*;
    use serde::Deserialize;

    pub struct BincodeHandler<I, R>
    where
        I: for<'de> Deserialize<'de>,
        R: Serialize,
    {
        endpoint: Endpoint,
        handler: Box<dyn HandlerTrait<I, R>>,
    }

    impl<I, R> BincodeHandler<I, R>
    where
        I: for<'de> Deserialize<'de>,
        R: Serialize,
    {
        pub fn new<H>(endpoint: impl Into<Endpoint>, handler: H) -> Self
        where
            H: HandlerTrait<I, R> + 'static,
        {
            BincodeHandler {
                endpoint: endpoint.into(),
                handler: Box::new(handler),
            }
        }
    }

    impl<I, R> APICallHandler for BincodeHandler<I, R>
    where
        I: for<'de> Deserialize<'de>,
        R: Serialize,
    {
        fn endpoint(&self) -> Endpoint {
            self.endpoint
        }

        fn handle(&self, src: PluginRid, payload: Vec<u8>) -> CallFut {
            match bincode::deserialize(&payload) {
                Ok(data) => {
                    let fut = self.handler.handle(src, data);
                    Box::pin(
                        async move { bincode::serialize(&fut.await?).map_err(APIError::other) },
                    )
                }
                Err(e) => Box::pin(future::ready(Err(APIError::other(e)))),
            }
        }
    }

    pub trait BincodeAPICall: serde::Serialize {
        fn endpoint(&self) -> Endpoint;
    }

    impl<T: BincodeAPICall> TryInto<APICall> for T {
        type Error = bincode::Error;

        fn try_into(self) -> Result<APICall, Self::Error> {
            Ok(APICall {
                endpoint: self.endpoint(),
                payload: bincode::serialize(&self)?,
            })
        }
    }
}

#[cfg(feature = "bincode")]
pub use deser_handler::*;

type Handlers = Arc<tokio::sync::RwLock<FxHashMap<Endpoint, Box<dyn APICallHandler>>>>;

#[derive(Default)]
pub struct APIRouter {
    handlers: Handlers,
}

impl APIRouter {
    pub async fn register(&mut self, handler: impl APICallHandler + 'static) {
        self.handlers
            .write()
            .await
            .insert(handler.endpoint(), Box::new(handler));
    }

    pub async fn handle(&self, src: PluginRid, call: APICall) -> Result<Vec<u8>, APIError> {
        let APICall {
            endpoint, payload, ..
        } = call;

        if let Some(handler) = self.handlers.read().await.get(&endpoint) {
            let result = handler.handle(src, payload).await?;
            Ok(result)
        } else {
            Err(APIError::EndpointNotFound(endpoint))
        }
    }

    pub async fn is_registered(&self, endpoint: Endpoint) -> bool {
        self.handlers.read().await.contains_key(&endpoint)
    }
}
