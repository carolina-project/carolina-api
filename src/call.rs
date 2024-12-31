use std::{future::Future, pin::Pin, sync::Arc};

use fxhash::FxHashMap;
use serde::Serialize;

use crate::{context::*, PinBoxFut};

pub type CallFut<'a> = Pin<Box<dyn Future<Output = APIResult> + Send + 'a>>;

pub trait APICallHandler: Send + Sync {
    fn endpoint(&self) -> Endpoint;

    fn handle(&self, src: PluginRid, payload: Vec<u8>) -> CallFut;
}

pub trait HandlerTrait<I, R>: Send + Sync {
    fn handle(&self, src: PluginRid, input: I) -> PinBoxFut<Result<R, APIError>>;
}

impl<'a, I, R, F, FR> HandlerTrait<I, R> for F
where
    F: Fn(PluginRid, I) -> FR + Send + Sync + 'a,
    FR: Future<Output = Result<R, APIError>> + Send + 'a,
    I: Send + 'static,
{
    fn handle(&self, src: PluginRid, input: I) -> PinBoxFut<Result<R, APIError>> {
        Box::pin(async move { (self)(src, input).await })
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
        type Output: for<'de> Deserialize<'de>;

        fn endpoint(&self) -> Endpoint;
    }

    impl<G: GlobalContext> PluginContext<G> {
        pub async fn call_bincode_api<C: BincodeAPICall>(
            &self,
            target: PluginRid,
            call: C,
        ) -> Result<C::Output, APIError> {
            let resp = self.call_api(target, call).await?;
            bincode::deserialize(&resp).map_err(APIError::other)
        }
    }

    impl<T: BincodeAPICall> IntoAPICall for T {
        type Error = bincode::Error;

        fn into_api_call(self) -> Result<APICall, Self::Error> {
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

#[derive(Debug, thiserror::Error)]
pub enum RegError {
    #[error("already registered, {0:?}")]
    Conflicted(Endpoint),
}

impl APIRouter {
    pub async fn register(
        &mut self,
        handler: impl APICallHandler + 'static,
    ) -> Result<(), RegError> {
        let handlers = self.handlers.write().await;
        let endpoint = handler.endpoint();
        if handlers.contains_key(&endpoint) {
            Err(RegError::Conflicted(endpoint))
        } else {
            self.handlers
                .write()
                .await
                .insert(handler.endpoint(), Box::new(handler));
            Ok(())
        }
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
