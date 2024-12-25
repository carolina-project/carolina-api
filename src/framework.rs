use std::{
    future::{self, Future},
    sync::Arc,
    task::ready,
};

use dashmap::DashMap;
use onebot_connect_interface::app::AppDyn;
use parking_lot::RwLock;

use crate::{context::*, CarolinaPlugin, PinBox};

type WithLock<P> = Arc<RwLock<P>>;

pub struct GlobalContextInner<P: CarolinaPlugin> {
    apps: DashMap<AppUid, Arc<dyn AppDyn>>,
    plugin_uid_map: DashMap<PluginUid, WithLock<P>>,
    plugin_id_map: DashMap<String, (PluginUid, WithLock<P>)>,
}

pub struct GlobalContextImpl<P> {
    inner: Arc<GlobalContextInner<P>>,
}

impl<P: CarolinaPlugin> GlobalContext for GlobalContextImpl<P> {
    fn get_app(&self, id: AppUid) -> Option<Box<dyn AppDyn>> {
        self.inner.apps.get(&id).map(|r| r.clone_app())
    }

    fn get_plugin_uid(&self, id: &str) -> Option<PluginUid> {
        self.inner.plugin_id_map.get(id).map(|r| r.0)
    }

    fn call_plugin_api(
        &self,
        src: PluginUid,
        call: APICall,
    ) -> impl Future<Output = APIResult> + Send + '_ {
        let plugin = match self.inner.plugin_uid_map.get(&src) {
            Some(plugin) => plugin,
            None => {
                return Box::pin(async move { Err(APIError::PluginNotFound(call.target)) }) as _
            }
        };
        let fut = plugin
            .read()
            .handle_api_call(src, call.endpoint, call.payload);

        Box::pin(async move { fut.await }) as _
    }
}
