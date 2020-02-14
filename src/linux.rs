use std::{
    io,
    os::unix::io::{AsRawFd, RawFd},
};
mod netlink;
mod rtnetlink;
mod sys;
pub use netlink::NetlinkIterator;

pub struct NetlinkSocket {
    fd: RawFd,
    address: libc::sockaddr_nl,
    seqnum: u32,
}

#[allow(non_snake_case)]
const fn CMSG_ALIGN(len: usize) -> usize {
    (len + size_of!(usize) - 1) & !(size_of!(usize) - 1)
}

#[allow(non_snake_case)]
const fn CMSG_SPACE(len: usize) -> usize {
    CMSG_ALIGN(len) + CMSG_ALIGN(size_of!(libc::cmsghdr))
}

impl NetlinkSocket {
    pub fn new() -> io::Result<Self> {
        let fd = unsafe {
            libc::socket(
                libc::AF_NETLINK,
                libc::SOCK_RAW | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC,
                libc::NETLINK_ROUTE,
            )
        };
        if fd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        unsafe {
            let mut address = std::mem::zeroed::<libc::sockaddr_nl>();
            address.nl_family = libc::AF_NETLINK as _;
            let bind_result = libc::bind(
                fd,
                &mut address as *mut _ as *mut libc::sockaddr,
                size_of!(libc::sockaddr_nl) as _,
            );
            if bind_result < 0 {
                libc::close(fd);
                return Err(std::io::Error::last_os_error());
            }
            let flag: libc::c_int = 1;
            let setsockopt_result = libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_PASSCRED,
                &flag as *const _ as *const _,
                size_of!(libc::c_int) as libc::socklen_t,
            );
            if setsockopt_result < 0 {
                libc::close(fd);
                panic!("setting SO_PASSCRED on a Netlink socket will succeed")
            }
            Ok(Self {
                fd,
                address,
                seqnum: 0,
            })
        }
    }

    pub fn send(&mut self) -> std::io::Result<()> {
        const SPACE: usize = netlink::NLMSG_SPACE(RTMSG_SIZE);
        use netlink::NETLINK_SIZE;
        #[repr(C)]
        struct Nlmsg {
            hdr: libc::nlmsghdr,
            msg: sys::rtnetlink::rtmsg,
        };
        if false {
            let _: [u8; size_of!(Nlmsg)] = [0u8; NETLINK_SIZE + RTMSG_SIZE];
            let _: [u8; SPACE] = [0u8; NETLINK_SIZE + RTMSG_SIZE];
        }

        let msg = Nlmsg {
            hdr: libc::nlmsghdr {
                nlmsg_len: SPACE as _,
                nlmsg_type: libc::RTM_GETROUTE as _,
                nlmsg_flags: (libc::NLM_F_REQUEST | libc::NLM_F_MULTI | libc::NLM_F_DUMP) as _,
                nlmsg_seq: self.seqnum,
                nlmsg_pid: self.address.nl_pid,
            },
            msg: sys::rtnetlink::rtmsg {
                rtm_family: libc::AF_UNSPEC as _,
                rtm_dst_len: 0,
                rtm_src_len: 0,
                rtm_tos: 0,
                rtm_protocol: sys::rtnetlink::RTPROT_UNSPEC as _,
                rtm_table: libc::RT_TABLE_LOCAL as _,
                rtm_scope: libc::RT_SCOPE_HOST as _,
                rtm_type: sys::rtnetlink::RTN_LOCAL as _,
                rtm_flags: sys::rtnetlink::RTM_F_NOTIFY as _,
            },
        };

        let status = unsafe {
            libc::sendto(
                self.fd,
                &msg as *const _ as *const _,
                SPACE as _,
                libc::MSG_NOSIGNAL,
                &self.address as *const _ as *const _,
                std::mem::size_of_val(&self.address) as _,
            )
        };

        if status == SPACE as _ {
            self.seqnum += 1;
            Ok(())
        } else if status == -1 {
            Err(std::io::Error::last_os_error())
        } else {
            unreachable!("datagram sockets do not send partial messages")
        }
    }

    pub fn recv(&self, buf: &mut [u32]) -> std::io::Result<usize> {
        let mut address = std::mem::MaybeUninit::<libc::sockaddr_nl>::uninit();
        #[repr(C)]
        struct UcredCmsg {
            hdr: libc::cmsghdr,
            data: libc::ucred,
        }

        if false {
            let _: [u8; size_of!(UcredCmsg)] = [0u8; CMSG_SPACE(size_of!(libc::ucred))];
        }
        let mut cmsghdr = std::mem::MaybeUninit::<UcredCmsg>::zeroed();
        let mut iovec = libc::iovec {
            iov_base: buf.as_mut_ptr() as *mut std::ffi::c_void,
            iov_len: buf.len() * size_of!(u32),
        };
        let mut msghdr = libc::msghdr {
            msg_name: &mut address as *mut _ as *mut _,
            msg_namelen: size_of!(libc::sockaddr_nl) as u32,
            msg_iov: &mut iovec,
            msg_iovlen: 1,
            msg_control: cmsghdr.as_mut_ptr() as *mut _,
            msg_controllen: size_of!(UcredCmsg),
            msg_flags: 0,
        };

        loop {
            let status = unsafe {
                libc::recvmsg(
                    self.fd,
                    &mut msghdr as *mut _,
                    libc::MSG_NOSIGNAL | libc::MSG_CMSG_CLOEXEC | libc::MSG_TRUNC,
                )
            };

            if status < 0 {
                break Err(std::io::Error::last_os_error());
            }
            assert_eq!(
                msghdr.msg_namelen,
                size_of!(libc::sockaddr_nl) as u32,
                "wrong size of message header"
            );
            let address = unsafe { address.assume_init() };
            assert_eq!(
                address.nl_family,
                libc::AF_NETLINK as u16,
                "kernel provided wrong address family"
            );
            assert_eq!(
                msghdr.msg_controllen,
                size_of!(UcredCmsg),
                "kernel didn’t provide a ucred ancillary data"
            );
            let cmsghdr = unsafe {
                let pointer = libc::CMSG_FIRSTHDR(&msghdr as *const _);
                assert_eq!(
                    pointer as *const libc::c_void,
                    cmsghdr.as_ptr() as *const libc::c_void,
                    "kernel provided bad ancillary data: incorrect start"
                );
                &*cmsghdr.as_ptr()
            };
            assert_eq!(&cmsghdr.data as *const _ as *const libc::c_uchar, unsafe {
                libc::CMSG_DATA(cmsghdr as *const _ as *const _)
            });
            assert_eq!(
                cmsghdr.hdr.cmsg_len,
                unsafe { libc::CMSG_LEN(size_of!(libc::ucred) as _) as _ },
                "kernel provided bad ancillary data: incorrect length"
            );
            assert_eq!(
                cmsghdr.hdr.cmsg_level,
                libc::SOL_SOCKET,
                "kernel provided bad ancillary data: incorrect level"
            );
            assert_eq!(
                cmsghdr.hdr.cmsg_type,
                libc::SCM_CREDENTIALS,
                "kernel provided bad ancillary data: incorrect type"
            );
            if address.nl_pid == 0
                && cmsghdr.data.pid == 0
                && cmsghdr.data.uid == 0
                && cmsghdr.data.gid == 0
            {
                break Ok(status as usize);
            } /* else: ignore packet not from kernel */
        }
    }
}

const RTMSG_SIZE: usize = size_of!(sys::rtnetlink::rtmsg);

impl AsRawFd for NetlinkSocket {
    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}

impl Drop for NetlinkSocket {
    fn drop(&mut self) {
        if unsafe { libc::close(self.fd) } != 0 {
            panic!("Closing a netlink socket will not fail")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn it_works() {
        let mut sock = NetlinkSocket::new().unwrap();
        sock.send().unwrap();
        let buf = &mut [0u32; 8192];
        while let Ok(len) = sock.recv(buf) {
            println!("Message was {} bytes", len);
            for i in NetlinkIterator::new(buf, len) {
                println!("Got {:?}", i)
            }
        }
    }
}
