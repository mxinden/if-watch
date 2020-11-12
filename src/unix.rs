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

#[cfg(not(target_os = "linux"))]
mod bsd;

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

#[derive(Debug)]
#[must_use]
enum Status<T> {
    IO(std::io::Error),
    Desync,
    Data(T),
}

/// An address set/watcher
#[derive(Debug)]
pub struct IfWatcher {
    hash: HashSet<IpNet>,
    watcher: Watcher,
    buf: Vec<u64>,
    queue: VecDeque<IfEvent>,
}

impl IfWatcher {
    /// Create a watcher
    pub async fn new() -> Result<Self> {
        let mut hash = HashSet::new();
        let mut watcher = Watcher::new()?;
        let mut buf = Vec::with_capacity(1 << 16);
        let mut queue = VecDeque::new();
        watcher.resync(&mut buf, &mut queue, &mut hash).await?;
        Ok(Self {
            hash,
            watcher,
            buf,
            queue,
        })
    }

    pub fn iter(&self) -> impl Iterator<Item = &IpNet> {
        self.hash.iter()
    }

    /// Returns a future for the next event.
    pub async fn next(&mut self) -> Result<IfEvent> {
        let Self {
            watcher,
            buf,
            hash,
            queue,
        } = self;
        if let Some(event) = queue.pop_front() {
            return Ok(event);
        }
        loop {
            match watcher.next(buf, queue, hash).await {
                Status::IO(e) => return Err(e),
                Status::Desync => {
                    if buf.capacity() < 1 << 19 {
                        buf.reserve(buf.capacity() * 2);
                    }
                    if watcher.resync(buf, queue, hash).await.is_err() {
                        continue;
                    }
                }
                Status::Data(()) => {
                    if let Some(event) = queue.pop_front() {
                        return Ok(event);
                    }
                }
            }
        }
    }
}
