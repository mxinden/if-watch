#![allow(unsafe_code)]
#[cfg(not(windows))]
compile_error!("this module only supports Windows!");

use crate::Event;
use futures_lite::future::poll_fn;
use std::{collections::{HashSet, VecDeque},
          net::IpAddr,
          sync::{atomic::{AtomicBool, Ordering},
                 Arc},
          task::{Poll, Waker}};
use winapi::shared::{netioapi::{CancelMibChangeNotify2, NotifyIpInterfaceChange,
                                MIB_IPINTERFACE_ROW, MIB_NOTIFICATION_TYPE},
                     ntdef::{HANDLE, PVOID},
                     winerror::NO_ERROR,
                     ws2def::AF_UNSPEC};

/// An address set/watcher
#[derive(Debug)]
pub struct AddrSet {
    addrs: HashSet<IpAddr>,
    queue: VecDeque<Event>,
    waker: Option<Waker>,
    notif: RouteChangeNotification,
    resync: Arc<AtomicBool>,
}

impl AddrSet {
    /// Create a watcher
    pub async fn new() -> std::io::Result<Self> {
        let resync = Arc::new(AtomicBool::new(true));
        Ok(Self {
            addrs: Default::default(),
            queue: Default::default(),
            waker: Default::default(),
            resync: resync.clone(),
            notif: RouteChangeNotification::new(Box::new(move |_, _| {
                resync.store(true, Ordering::SeqCst);
            }))?,
        })
    }

    fn resync(&mut self) -> std::io::Result<()> {
        let addrs = if_addrs::get_if_addrs()?;
        for old_addr in self.addrs.clone() {
            if addrs.iter().find(|addr| addr.ip() == old_addr).is_none() {
                self.addrs.remove(&old_addr);
                self.queue.push_back(Event::Delete(old_addr));
            }
        }
        for new_addr in addrs {
            let ip = new_addr.ip();
            if self.addrs.insert(ip) {
                self.queue.push_back(Event::New(ip));
            }
        }
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
        Ok(())
    }

    /// Returns a future for the next event.
    pub async fn next(&mut self) -> std::io::Result<Event> {
        poll_fn(|cx| {
            self.waker = Some(cx.waker().clone());
            if self.resync.load(Ordering::SeqCst) {
                if let Err(error) = self.resync() {
                    return Poll::Ready(Err(error))
                }
            }
            if let Some(event) = self.queue.pop_front() {
                Poll::Ready(Ok(event))
            } else {
                Poll::Pending
            }
        })
        .await
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

#[cfg(test)]
mod tests {
    use super::*;
    use winapi::{shared::minwindef::DWORD,
                 um::{processthreadsapi::GetCurrentThreadId, synchapi::SleepEx}};

    fn get_current_thread_id() -> DWORD { unsafe { GetCurrentThreadId() } }
    #[test]
    fn does_not_hang_forever() {
        println!("Current thread ID is {}", get_current_thread_id());
        let _i = RouteChangeNotification::new(Box::new(|_, _| {
            println!("Got a notification on thread {}", get_current_thread_id())
        }))
        .unwrap();
        unsafe {
            SleepEx(1000000, 1);
        }
    }
}
