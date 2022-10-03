//! IP address watching.
#![deny(missing_docs)]
#![deny(warnings)]

#[cfg(not(target_os = "linux"))]
use futures::stream::{FusedStream, Stream};
pub use ipnet::{IpNet, Ipv4Net, Ipv6Net};
#[cfg(not(target_os = "linux"))]
use std::{
    io::Result,
    pin::Pin,
    task::{Context, Poll},
};

#[cfg(target_os = "macos")]
mod apple;
#[cfg(target_os = "ios")]
mod apple;
#[cfg(not(any(
    target_os = "ios",
    target_os = "linux",
    target_os = "macos",
    target_os = "windows",
)))]
mod fallback;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "windows")]
mod win;

#[cfg(target_os = "macos")]
use apple as platform_impl;
#[cfg(target_os = "ios")]
use apple as platform_impl;
#[cfg(not(any(
    target_os = "ios",
    target_os = "linux",
    target_os = "macos",
    target_os = "windows",
)))]
use fallback as platform_impl;
#[cfg(target_os = "windows")]
use win as platform_impl;

#[cfg(target_os = "linux")]
#[cfg(feature = "tokio")]
pub use linux::tokio;

#[cfg(target_os = "linux")]
#[cfg(feature = "smol")]
pub use linux::smol::IfWatcher;

/// An address change event.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum IfEvent {
    /// A new local address has been added.
    Up(IpNet),
    /// A local address has been deleted.
    Down(IpNet),
}

/// Watches for interface changes.
#[cfg(not(target_os = "linux"))]
#[derive(Debug)]
pub struct IfWatcher(platform_impl::IfWatcher);

#[cfg(not(target_os = "linux"))]
impl IfWatcher {
    /// Create a watcher.
    pub fn new() -> Result<Self> {
        platform_impl::IfWatcher::new().map(Self)
    }

    /// Iterate over current networks.
    pub fn iter(&self) -> impl Iterator<Item = &IpNet> {
        self.0.iter()
    }

    /// Poll for an address change event.
    pub fn poll_if_event(&mut self, cx: &mut Context) -> Poll<Result<IfEvent>> {
        self.0.poll_if_event(cx)
    }
}

#[cfg(not(target_os = "linux"))]
impl Stream for IfWatcher {
    type Item = Result<IfEvent>;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::into_inner(self).poll_if_event(cx).map(Some)
    }
}

#[cfg(not(target_os = "linux"))]
impl FusedStream for IfWatcher {
    fn is_terminated(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::IfWatcher;
    use futures::StreamExt;
    use std::pin::Pin;

    #[test]
    fn test_ip_watch() {
        futures::executor::block_on(async {
            let mut set = IfWatcher::new().unwrap();
            let event = set.select_next_some().await.unwrap();
            println!("Got event {:?}", event);
        });
    }

    #[test]
    fn test_is_send() {
        futures::executor::block_on(async {
            fn is_send<T: Send>(_: T) {}
            is_send(IfWatcher::new());
            is_send(IfWatcher::new().unwrap());
            is_send(Pin::new(&mut IfWatcher::new().unwrap()));
        });
    }
}
