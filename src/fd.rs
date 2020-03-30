use std::os::unix::prelude::*;
pub(crate) struct Fd {
    fd: RawFd,
}
const FLAGS: i32 = libc::SOCK_RAW | libc::SOCK_CLOEXEC | libc::SOCK_NONBLOCK;
impl Fd {
    pub(super) fn new() -> std::io::Result<Fd> {
        #[cfg(target_os = "linux")]
        let fd = unsafe { libc::socket(libc::PF_NETLINK, FLAGS, libc::NETLINK_ROUTE) };
        #[cfg(not(target_os = "linux"))]
        let fd = unsafe { libc::socket(libc::PF_ROUTE, FLAGS, libc::AF_UNSPEC) };
        if fd < 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(Fd { fd })
        }
    }
}

impl AsRawFd for Fd {
    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}

impl Drop for Fd {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.fd);
        }
    }
}
