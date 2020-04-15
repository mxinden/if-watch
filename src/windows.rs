#![allow(unsafe_code)]
#[cfg(not(windows))]
compile_error!("this module only supports Windows!");

#[allow(
    nonstandard_style,
    trivial_numeric_casts,
    trivial_casts,
    unsafe_code,
    unused,
    unreachable_pub,
    missing_docs
)]
mod bindings;
use bindings::*;

#[link(name = "iphlpapi")]
extern "C" {}

/// Route change notifications
#[allow(missing_debug_implementations)]
pub struct RouteChangeNotification {
    handle: HANDLE,
    callback: *mut RouteChangeCallback,
    // actual callback follows
}

#[cfg(any())]
fn align_to<T, U>() -> Option<(Layout, usize)> {
    let layout_t = Layout::new::<T>();
    let layout_u = Layout::new::<U>();
    assert!(layout_t.size() >= layout_t.align());
    let align = std::cmp::max(layout_t.size(), layout_u.align());
    if (usize::MAX >> 1).checked_sub(layout_u.size())? < align {
        None
    } else {
        unsafe { Layout::from_size_align_unchecked(align + layout_u.size(), align) }
    }
}

/// The type of route change callbacks
pub type RouteChangeCallback = Box<dyn FnMut(&MIB_IPFORWARD_ROW2, MIB_NOTIFICATION_TYPE) + Send>;
impl RouteChangeNotification {
    /// Register for route change notifications
    pub fn new(cb: RouteChangeCallback) -> Result<Self, ()> {
        #[allow(non_snake_case)]
        unsafe extern "stdcall" fn global_callback(
            CallerContext: PVOID,
            Row: PMIB_IPFORWARD_ROW2,
            NotificationType: MIB_NOTIFICATION_TYPE,
        ) {
            (**(CallerContext as *mut RouteChangeCallback))(&*Row, NotificationType)
        }
        let mut handle = core::ptr::null_mut();
        let callback = Box::into_raw(Box::new(cb));
        if unsafe {
            NotifyRouteChange2(
                AF_UNSPEC as _,
                Some(global_callback),
                callback as _,
                0,
                &mut handle,
            )
        } != NO_ERROR as _
        {
            Err(())
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

#[cfg(test)]
mod tests {
    use super::*;

    fn get_current_thread_id() -> DWORD {
        unsafe { GetCurrentThreadId() }
    }
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
