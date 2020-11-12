//! IP address watching.
#![deny(missing_docs)]
#![deny(warnings)]

use ipnet::IpNet;
use std::io::Result;

#[cfg(not(any(unix, windows)))]
compile_error!("Only Unix and Windows are supported");

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
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

    /// Returns a future for the next event.
    pub async fn next(&mut self) -> Result<IfEvent> {
        self.0.next().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_lite::future::poll_fn;
    use std::{future::Future, pin::Pin, task::Poll};

    #[test]
    fn test_ip_watch() {
        futures_lite::future::block_on(async {
            let mut set = IfWatcher::new().await.unwrap();
            poll_fn(|cx| loop {
                let next = set.next();
                futures_lite::pin!(next);
                if let Poll::Ready(Ok(ev)) = Pin::new(&mut next).poll(cx) {
                    println!("Got event {:?}", ev);
                    continue;
                }
                return Poll::Ready(());
            })
            .await;
        });
    }

    #[test]
    fn test_is_send() {
        futures_lite::future::block_on(async {
            fn is_send<T: Send>(_: T) {}
            is_send(IfWatcher::new());
            is_send(IfWatcher::new().await.unwrap());
            is_send(IfWatcher::new().await.unwrap().next());
        });
    }
}
