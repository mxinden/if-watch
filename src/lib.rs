//! IP address watching.
#![deny(missing_docs)]
#![deny(warnings)]

pub use ipnet::IpNet;
use std::future::Future;
use std::io::Result;
use std::pin::Pin;
use std::task::{Context, Poll};

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
pub enum IfWatcher {
    /// Uses platform api.
    Platform(platform_impl::IfWatcher),
    /// Polling fallback.
    Fallback(fallback::IfWatcher),
}

impl IfWatcher {
    /// Create a watcher
    pub async fn new() -> Result<Self> {
        if std::env::var_os("WSL_DISTRO_NAME").is_none() {
            Ok(Self::Platform(platform_impl::IfWatcher::new().await?))
        } else {
            Ok(Self::Fallback(fallback::IfWatcher::new().await?))
        }
    }

    /// Iterate over current networks.
    pub fn iter(&self) -> impl Iterator<Item = &IpNet> {
        match self {
            Self::Platform(watcher) => watcher.iter(),
            Self::Fallback(watcher) => watcher.iter(),
        }
    }
}

impl Future for IfWatcher {
    type Output = Result<IfEvent>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        match &mut *self {
            Self::Platform(watcher) => Pin::new(watcher).poll(cx),
            Self::Fallback(watcher) => Pin::new(watcher).poll(cx),
        }
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
