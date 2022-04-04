use crate::{IfEvent, IpNet, Ipv4Net, Ipv6Net};
use fnv::FnvHashSet;
use futures::task::AtomicWaker;
use if_addrs::IfAddr;
use std::collections::VecDeque;
use std::ffi::c_void;
use std::future::Future;
use std::io::{Error, ErrorKind, Result};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use windows::Win32::Foundation::{BOOLEAN, HANDLE};
use windows::Win32::NetworkManagement::IpHelper::{
    CancelMibChangeNotify2, NotifyIpInterfaceChange, AF_UNSPEC, MIB_IPINTERFACE_ROW,
    MIB_NOTIFICATION_TYPE,
};

/// An address set/watcher
#[derive(Debug)]
pub struct IfWatcher {
    addrs: FnvHashSet<IpNet>,
    queue: VecDeque<IfEvent>,
    #[allow(unused)]
    notif: IpChangeNotification,
    waker: Arc<AtomicWaker>,
    resync: Arc<AtomicBool>,
}

impl IfWatcher {
    /// Create a watcher
    pub async fn new() -> Result<Self> {
        let resync = Arc::new(AtomicBool::new(true));
        let waker = Arc::new(AtomicWaker::new());
        Ok(Self {
            addrs: Default::default(),
            queue: Default::default(),
            waker: waker.clone(),
            resync: resync.clone(),
            notif: IpChangeNotification::new(Box::new(move |_, _| {
                resync.store(true, Ordering::Relaxed);
                waker.wake();
            }))?,
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
}

impl Future for IfWatcher {
    type Output = Result<IfEvent>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        self.waker.register(cx.waker());
        if self.resync.swap(false, Ordering::Relaxed) {
            if let Err(error) = self.resync() {
                return Poll::Ready(Err(error));
            }
        }
        if let Some(event) = self.queue.pop_front() {
            Poll::Ready(Ok(event))
        } else {
            Poll::Pending
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

/// IP change notifications
struct IpChangeNotification {
    handle: HANDLE,
    callback: *mut IpChangeCallback,
}

impl std::fmt::Debug for IpChangeNotification {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "IpChangeNotification")
    }
}

type IpChangeCallback = Box<dyn Fn(&MIB_IPINTERFACE_ROW, MIB_NOTIFICATION_TYPE) + Send>;

impl IpChangeNotification {
    /// Register for route change notifications
    fn new(cb: IpChangeCallback) -> Result<Self> {
        unsafe extern "system" fn global_callback(
            caller_context: *const c_void,
            row: *const MIB_IPINTERFACE_ROW,
            notification_type: MIB_NOTIFICATION_TYPE,
        ) {
            (**(caller_context as *const IpChangeCallback))(&*row, notification_type)
        }
        let mut handle = HANDLE::default();
        let callback = Box::into_raw(Box::new(cb));
        unsafe {
            NotifyIpInterfaceChange(
                AF_UNSPEC.0 as _,
                Some(global_callback),
                callback as _,
                BOOLEAN(0),
                &mut handle as _,
            )
            .map_err(|err| Error::new(ErrorKind::Other, err.to_string()))?;
        }
        Ok(Self { callback, handle })
    }
}

impl Drop for IpChangeNotification {
    fn drop(&mut self) {
        unsafe {
            if let Err(err) = CancelMibChangeNotify2(self.handle) {
                log::error!("error deregistering notification: {}", err);
            }
            drop(Box::from_raw(self.callback));
        }
    }
}

unsafe impl Send for IpChangeNotification {}
