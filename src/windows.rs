use crate::IfEvent;
use futures::task::AtomicWaker;
use if_addrs::IfAddr;
use ipnet::{IpNet, Ipv4Net, Ipv6Net};
use std::{
    collections::{HashSet, VecDeque},
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    task::{Context, Poll},
};
use winapi::shared::{
    netioapi::{
        CancelMibChangeNotify2, NotifyIpInterfaceChange, MIB_IPINTERFACE_ROW, MIB_NOTIFICATION_TYPE,
    },
    ntdef::{HANDLE, PVOID},
    winerror::NO_ERROR,
    ws2def::AF_UNSPEC,
};

/// An address set/watcher
#[derive(Debug)]
pub struct IfWatcher {
    addrs: HashSet<IpNet>,
    queue: VecDeque<IfEvent>,
    notif: RouteChangeNotification,
    waker: Arc<AtomicWaker>,
    resync: Arc<AtomicBool>,
}

impl IfWatcher {
    /// Create a watcher
    pub async fn new() -> std::io::Result<Self> {
        let resync = Arc::new(AtomicBool::new(true));
        let waker = Arc::new(AtomicWaker::new());
        Ok(Self {
            addrs: Default::default(),
            queue: Default::default(),
            waker: waker.clone(),
            resync: resync.clone(),
            notif: RouteChangeNotification::new(Box::new(move |_, _| {
                resync.store(true, Ordering::Relaxed);
                waker.wake();
            }))?,
        })
    }

    fn resync(&mut self) -> std::io::Result<()> {
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

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
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

/// Route change notifications
#[derive(Debug)]
struct RouteChangeNotification {
    handle: HANDLE,
    callback: *mut RouteChangeCallback,
    // actual callback follows
}

/// The type of route change callbacks
type RouteChangeCallback = Box<dyn Fn(&MIB_IPINTERFACE_ROW, MIB_NOTIFICATION_TYPE) + Send>;
impl RouteChangeNotification {
    /// Register for route change notifications
    fn new(cb: RouteChangeCallback) -> std::io::Result<Self> {
        #[allow(non_snake_case)]
        unsafe extern "system" fn global_callback(
            CallerContext: PVOID,
            Row: *mut MIB_IPINTERFACE_ROW,
            NotificationType: MIB_NOTIFICATION_TYPE,
        ) {
            (**(CallerContext as *const RouteChangeCallback))(&*Row, NotificationType)
        }
        let mut handle = core::ptr::null_mut();
        let callback = Box::into_raw(Box::new(cb));
        if unsafe {
            NotifyIpInterfaceChange(
                AF_UNSPEC as _,
                Some(global_callback),
                callback as _,
                0,
                &mut handle,
            )
        } != NO_ERROR
        {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(Self { callback, handle })
        }
    }
}

impl Drop for RouteChangeNotification {
    fn drop(&mut self) {
        unsafe {
            CancelMibChangeNotify2(self.handle);
            drop(Box::from_raw(self.callback));
        }
    }
}

unsafe impl Send for RouteChangeNotification {}
