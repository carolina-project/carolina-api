use crate::*;

use dashmap::DashMap;
use std::{
    cell::UnsafeCell, future::Future, io, ops::{Deref, DerefMut}, path::PathBuf, pin::Pin, str::FromStr, sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    }, task::{Poll, Waker}
};

#[derive(Default, Debug)]
pub struct EventMapper {
    type2uid: DashMap<String, DashMap<String, Vec<PluginRid>>>,
    uid2type: DashMap<PluginRid, Vec<(String, Option<String>)>>,
}

impl EventMapper {
    pub fn subscribe(&self, types: Vec<(String, Option<String>)>, rid: PluginRid) {
        for (ty, detail_ty) in &types {
            self.type2uid
                .entry(ty.clone())
                .or_default()
                .entry(detail_ty.clone().unwrap_or_default())
                .or_default()
                .push(rid);
        }

        self.uid2type.insert(rid, types);
    }

    pub fn filter_plugins(
        &self,
        ty: impl AsRef<str>,
        detail_ty: impl AsRef<str>,
    ) -> Vec<PluginRid> {
        self.type2uid
            .get(ty.as_ref())
            .map(|map| {
                let mut collected = map
                    .get(detail_ty.as_ref())
                    .map(|r| r.clone())
                    .unwrap_or_default();
                if let Some(sub) = map.get("") {
                    sub.iter().for_each(|r| collected.push(*r));
                }
                collected
            })
            .unwrap_or_default()
    }
}

pub struct DirConfig {
    pub config_path: PathBuf,
    pub data_path: PathBuf,
}
impl DirConfig {
    pub fn new(config: Option<PathBuf>, data: Option<PathBuf>) -> Self {
        let config_path = config.unwrap_or_else(|| {
            dirs::config_dir()
                .unwrap_or_else(|| PathBuf::from_str(".").unwrap())
                .join("carolina")
        });
        let data_path = data.unwrap_or_else(|| {
            dirs::data_dir()
                .unwrap_or_else(|| PathBuf::from_str(".").unwrap())
                .join("carolina")
        });

        DirConfig {
            config_path,
            data_path,
        }
    }

    pub async fn ensure_dirs(&self) -> io::Result<()> {
        use tokio::fs;

        fs::create_dir_all(&self.config_path).await?;
        fs::create_dir_all(&self.data_path).await?;
        Ok(())
    }
}
impl Default for DirConfig {
    fn default() -> Self {
        Self::new(None, None)
    }
}

pub(super) struct UnsafePluginWrapper<P: CarolinaPlugin>(UnsafeCell<P>);

impl<P: CarolinaPlugin> UnsafePluginWrapper<P> {
    pub fn into_inner(self) -> P {
        self.0.into_inner()
    }
}

unsafe impl<P: CarolinaPlugin> Send for UnsafePluginWrapper<P> {}
unsafe impl<P: CarolinaPlugin> Sync for UnsafePluginWrapper<P> {}

impl<P: CarolinaPlugin> From<P> for UnsafePluginWrapper<P> {
    fn from(value: P) -> Self {
        Self(value.into())
    }
}
impl<P: CarolinaPlugin> Deref for UnsafePluginWrapper<P> {
    type Target = P;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.0.get() }
    }
}
impl<P: CarolinaPlugin> DerefMut for UnsafePluginWrapper<P> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.0.get() }
    }
}

#[derive(Debug, Default)]
struct CompletedInner {
    wakers: Mutex<Vec<Waker>>,
    state: AtomicBool,
}

#[derive(Debug, Default, Clone)]
pub struct Completed {
    inner: Arc<CompletedInner>,
}

impl Completed {
    pub fn complete(&self) {
        self.inner.state.store(true, Ordering::Release);

        let wakers = std::mem::take(&mut *self.inner.wakers.lock().unwrap());
        for ele in wakers.into_iter() {
            ele.wake();
        }
    }

    pub fn is_completed(&self) -> bool {
        self.inner.state.load(Ordering::Acquire)
    }

    pub fn wait(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl Future for Completed {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        if self.is_completed() {
            Poll::Ready(())
        } else {
            self.inner.wakers.lock().unwrap().push(cx.waker().clone());
            std::task::Poll::Pending
        }
    }
}
