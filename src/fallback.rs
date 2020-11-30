use crate::IfEvent;
use async_io::Timer;
use futures_lite::StreamExt;
use if_addrs::IfAddr;
use ipnet::{IpNet, Ipv4Net, Ipv6Net};
use std::io::Result;
use std::{
    collections::{HashSet, VecDeque},
    time::{Duration, Instant},
};

/// An address set/watcher
#[derive(Debug)]
pub struct IfWatcher {
    addrs: HashSet<IpNet>,
    queue: VecDeque<IfEvent>,
    ticker: Timer,
}

impl IfWatcher {
    /// Create a watcher
    pub async fn new() -> Result<Self> {
        Ok(Self {
            addrs: Default::default(),
            queue: Default::default(),
            ticker: Timer::interval_at(Instant::now(), Duration::from_secs(10)),
        })
    }

    fn resync(&mut self) -> Result<()> {
        let addrs = if_addrs::get_if_addrs()?;
        for old_addr in self.addrs.clone() {
            if addrs
                .iter()
                .find(|addr| addr.ip() == old_addr.addr())
                .is_none()
            {
                self.addrs.remove(&old_addr);
                self.queue.push_back(IfEvent::Down(old_addr));
            }
        }
        for new_addr in addrs {
            let ipnet = ifaddr_to_ipnet(new_addr.addr);
            if self.addrs.insert(ipnet) {
                self.queue.push_back(IfEvent::Up(ipnet));
            }
        }
        Ok(())
    }

    pub fn iter(&self) -> impl Iterator<Item = &IpNet> {
        self.addrs.iter()
    }

    /// Returns a future for the next event.
    pub async fn next(&mut self) -> Result<IfEvent> {
        loop {
            if let Some(event) = self.queue.pop_front() {
                return Ok(event);
            }
            self.ticker.next().await;
            self.resync()?;
        }
    }
}

fn ifaddr_to_ipnet(addr: IfAddr) -> IpNet {
    match addr {
        IfAddr::V4(ip) => {
            let prefix_len = (!u32::from_be_bytes(ip.netmask.octets())).leading_zeros();
            IpNet::V4(
                Ipv4Net::new(ip.ip, prefix_len as u8).expect("if_addrs returned a valid prefix"),
            )
        }
        IfAddr::V6(ip) => {
            let prefix_len = (!u128::from_be_bytes(ip.netmask.octets())).leading_zeros();
            IpNet::V6(
                Ipv6Net::new(ip.ip, prefix_len as u8).expect("if_addrs returned a valid prefix"),
            )
        }
    }
}
