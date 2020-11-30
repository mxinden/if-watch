macro_rules! size_of {
    ($t:ty) => {
        ::core::mem::size_of::<$t>()
    };
}

macro_rules! align_of {
    ($t:ty) => {
        ::core::mem::align_of::<$t>()
    };
}

macro_rules! errno {
    ($t:expr) => {{
        let res = $t;
        if res < 0 {
            Err(Error::last_os_error())
        } else {
            Ok(res)
        }
    }};
}

use crate::IfEvent;
use async_io::Async;
use ipnet::IpNet;
use std::collections::{HashSet, VecDeque};
use std::io::Result;
use std::os::unix::prelude::*;

mod aligned_buffer;

#[cfg(target_os = "linux")]
mod linux;
type Watcher = linux::NetlinkSocket;

#[derive(Debug)]
struct Fd(RawFd);

impl Fd {
    pub fn new(fd: RawFd) -> Result<Async<Self>> {
        Async::new(Self(fd))
    }
}

impl AsRawFd for Fd {
    fn as_raw_fd(&self) -> RawFd {
        self.0
    }
}

impl Drop for Fd {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.0);
        }
    }
}

/// An address set/watcher
#[derive(Debug)]
pub struct IfWatcher {
    addrs: HashSet<IpNet>,
    watcher: Watcher,
    queue: VecDeque<IfEvent>,
}

impl IfWatcher {
    /// Create a watcher
    pub async fn new() -> Result<Self> {
        let addrs = HashSet::new();
        let queue = VecDeque::new();
        let mut watcher = Watcher::new()?;
        watcher.send_getaddr().await?;
        Ok(Self {
            addrs,
            watcher,
            queue,
        })
    }

    /// Returns an iterator of ip's.
    pub fn iter(&self) -> impl Iterator<Item = &IpNet> {
        self.addrs.iter()
    }

    /// Returns a future for the next event.
    pub async fn next(&mut self) -> Result<IfEvent> {
        loop {
            while let Some(event) = self.queue.pop_front() {
                match event {
                    IfEvent::Up(inet) => {
                        if self.addrs.insert(inet) {
                            return Ok(event);
                        }
                    }
                    IfEvent::Down(inet) => {
                        if self.addrs.remove(&inet) {
                            return Ok(event);
                        }
                    }
                }
            }
            self.watcher.recv_event(&mut self.queue).await?;
        }
    }
}
