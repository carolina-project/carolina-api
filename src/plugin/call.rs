use std::{future::Future, pin::Pin, sync::Arc};

use fxhash::FxHashMap;
use oc_interface::value::{self, Value};
use serde::Serialize;

use crate::*;

pub type CallFut<'a> = Pin<Box<dyn Future<Output = APIResult> + Send + 'a>>;

pub trait APICallHandler: Send + Sync {
    fn endpoint(&self) -> Endpoint;

    fn handle(&self, src: PluginRid, payload: Value) -> CallFut;
}

/// A trait for handling API calls with input and output types.
pub trait HandlerTrait<I, R>: Send + Sync {
    fn handle(&self, src: PluginRid, input: I) -> PinBoxFut<Result<R, APIError>>;
}

impl<I, R, F, FR> HandlerTrait<I, R> for F
where
    F: Fn(PluginRid, I) -> FR + Send + Sync,
    FR: Future<Output = Result<R, APIError>> + Send,
    I: Send + 'static,
{
    fn handle(&self, src: PluginRid, input: I) -> PinBoxFut<Result<R, APIError>> {
        Box::pin(async move { (self)(src, input).await })
    }
}

pub struct FnHandler {
    endpoint: Endpoint,
    handler: Box<dyn HandlerTrait<Value, Value>>,
}

impl FnHandler {
    pub fn new<H>(endpoint: impl Into<Endpoint>, handler: H) -> Self
    where
        H: HandlerTrait<Value, Value> + 'static,
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

    fn handle(&self, src: PluginRid, payload: Value) -> CallFut {
        self.handler.handle(src, payload)
    }
}

mod serde_handler {
    use std::future;

    use super::*;
    use serde::Deserialize;

    pub struct SerdeHandler<I, R>
    where
        I: for<'de> Deserialize<'de>,
        R: Serialize,
    {
        endpoint: Endpoint,
        handler: Box<dyn HandlerTrait<I, R>>,
    }

    impl<I, R> SerdeHandler<I, R>
    where
        I: for<'de> Deserialize<'de>,
        R: Serialize,
    {
        pub fn new<H>(endpoint: impl Into<Endpoint>, handler: H) -> Self
        where
            H: HandlerTrait<I, R> + 'static,
        {
            SerdeHandler {
                endpoint: endpoint.into(),
                handler: Box::new(handler),
            }
        }
    }

    impl<I, R> APICallHandler for SerdeHandler<I, R>
    where
        I: for<'de> Deserialize<'de>,
        R: Serialize,
    {
        fn endpoint(&self) -> Endpoint {
            self.endpoint
        }

        fn handle(&self, src: PluginRid, payload: Value) -> CallFut {
            match I::deserialize(payload) {
                Ok(data) => {
                    let fut = self.handler.handle(src, data);
                    Box::pin(async move { value::to_value(fut.await?).map_err(APIError::other) })
                }
                Err(e) => Box::pin(future::ready(Err(APIError::other(e)))),
            }
        }
    }

    pub trait SerdeAPICall: serde::Serialize {
        type Output: for<'de> Deserialize<'de>;

        fn endpoint(&self) -> Endpoint;
    }

    impl<G: GlobalContext> PluginContext<G> {
        pub async fn call_serde_api<C: SerdeAPICall>(
            &self,
            target: PluginRid,
            call: C,
        ) -> Result<C::Output, APIError> {
            let resp = self.call_api(target, call).await?;
            C::Output::deserialize(resp).map_err(APIError::other)
        }
    }

    impl<T: SerdeAPICall> IntoAPICall for T {
        type Error = value::SerializerError;

        fn into_api_call(self) -> Result<APICall, Self::Error> {
            Ok(APICall {
                endpoint: self.endpoint(),
                payload: value::to_value(&self)?,
            })
        }
    }
}

pub use serde_handler::*;

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
        let mut handlers = self.handlers.write().await;
        let endpoint = handler.endpoint();
        if handlers.contains_key(&endpoint) {
            Err(RegError::Conflicted(endpoint))
        } else {
            handlers.insert(handler.endpoint(), Box::new(handler));
            Ok(())
        }
    }

    pub async fn handle(&self, src: PluginRid, call: APICall) -> Result<Value, APIError> {
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
