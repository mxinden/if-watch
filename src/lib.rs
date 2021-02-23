//! IP address watching.
#![deny(missing_docs)]
#![deny(warnings)]

pub use ipnet::IpNet;
use std::future::Future;
use std::io::Result;
use std::pin::Pin;
use std::task::{Context, Poll};

#[cfg(not(any(unix, windows)))]
compile_error!("Only Unix and Windows are supported");

#[cfg(not(any(target_os = "linux", windows)))]
mod fallback;
#[cfg(target_os = "linux")]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(not(any(target_os = "linux", windows)))]
use fallback as platform_impl;
#[cfg(target_os = "linux")]
use unix as platform_impl;
#[cfg(windows)]
use windows as platform_impl;

/// An address change event.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum IfEvent {
    /// A new local address has been added.
    Up(IpNet),
    /// A local address has been deleted.
    Down(IpNet),
}

/// Watches for interface changes.
#[derive(Debug)]
pub struct IfWatcher(platform_impl::IfWatcher);

impl IfWatcher {
    /// Create a watcher
    pub async fn new() -> Result<Self> {
        Ok(Self(platform_impl::IfWatcher::new().await?))
    }

    /// Iterate over current networks.
    pub fn iter(&self) -> impl Iterator<Item = &IpNet> {
        self.0.iter()
    }
}

impl Future for IfWatcher {
    type Output = Result<IfEvent>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        Pin::new(&mut self.0).poll(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ip_watch() {
        futures_lite::future::block_on(async {
            let mut set = IfWatcher::new().await.unwrap();
            let event = Pin::new(&mut set).await.unwrap();
            println!("Got event {:?}", event);
        });
    }

    #[test]
    fn test_is_send() {
        futures_lite::future::block_on(async {
            fn is_send<T: Send>(_: T) {}
            is_send(IfWatcher::new());
            is_send(IfWatcher::new().await.unwrap());
            is_send(Pin::new(&mut IfWatcher::new().await.unwrap()));
        });
    }
}
