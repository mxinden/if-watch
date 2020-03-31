macro_rules! size_of {
    ($t: ty) => {
        ::core::mem::size_of::<$t>()
    };
}

macro_rules! align_of {
    ($t: ty) => {
        ::core::mem::align_of::<$t>()
    };
}

use super::Event;
use std::{collections::HashSet, collections::VecDeque, net::IpAddr};
mod aligned_buffer;
mod fd;

#[cfg(target_os = "linux")]
mod linux;
type Watcher = linux::NetlinkSocket;

#[cfg(not(target_os = "linux"))]
mod bsd;

#[derive(Debug)]
#[must_use]
enum Status<T> {
    IO(std::io::Error),
    Desync,
    Data(T),
}

use fd::Fd as RoutingSocket;

/// An address set/watcher
#[derive(Debug)]
pub struct AddrSet {
    hash: HashSet<IpAddr>,
    watcher: Watcher,
    buf: Vec<u64>,
    queue: VecDeque<Event>,
}

impl AddrSet {
    /// Create a watcher
    pub fn new() -> std::io::Result<Self> {
        let mut hash = HashSet::new();
        let mut watcher = Watcher::new()?;
        let mut buf = Vec::with_capacity(1 << 16);
        let mut queue = VecDeque::new();
        watcher.resync(&mut buf, &mut queue, &mut hash)?;
        Ok(Self {
            hash,
            watcher,
            buf,
            queue,
        })
    }
}

impl Iterator for AddrSet {
    type Item = std::io::Result<Event>;
    fn next(&mut self) -> Option<Self::Item> {
        let Self {
            watcher,
            buf,
            hash,
            queue,
        } = self;
        if let Some(event) = queue.pop_front() {
            return Some(Ok(event));
        }
        loop {
            match watcher.next(buf, queue, hash) {
                Status::IO(e) => return Some(Err(e)),
                Status::Desync => {
                    if buf.capacity() < 1 << 20 {
                        buf.reserve(buf.capacity() * 2);
                    }
                    if watcher.resync(buf, queue, hash).is_err() {
                        continue;
                    }
                }
                Status::Data(()) => {
                    if let Some(event) = queue.pop_front() {
                        return Some(Ok(event));
                    }
                }
            }
        }
    }
}
