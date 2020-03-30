use std::os::unix::prelude::*;
mod netlink;
mod rtnetlink;
use super::{Error, Result};
pub use netlink::NetlinkIterator;
pub use rtnetlink::RtaIterator;

pub struct NetlinkSocket {
    fd: crate::RoutingSocket,
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

const RTMGRP_IPV4_ROUTE: u32 = 0x40;
const RTMGRP_IPV6_ROUTE: u32 = 0x400;
impl NetlinkSocket {
    pub fn new() -> Result<Self> {
        let fd = crate::RoutingSocket::new().map_err(Error::IO)?;
        unsafe {
            let mut address = std::mem::zeroed::<libc::sockaddr_nl>();
            address.nl_family = libc::AF_NETLINK as _;
            address.nl_groups = RTMGRP_IPV4_ROUTE | RTMGRP_IPV6_ROUTE;
            let bind_result = libc::bind(
                fd.as_raw_fd(),
                &mut address as *mut _ as *mut libc::sockaddr,
                size_of!(libc::sockaddr_nl) as _,
            );
            if bind_result < 0 {
                return Err(Error::IO(std::io::Error::last_os_error()));
            }
            address.nl_groups = 0;
            let flag: libc::c_int = 1;
            let setsockopt_result = libc::setsockopt(
                fd.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_PASSCRED,
                &flag as *const _ as *const _,
                size_of!(libc::c_int) as libc::socklen_t,
            );
            if setsockopt_result != 0 {
                return Err(Error::IO(std::io::Error::last_os_error()));
            }
            Ok(Self {
                fd,
                address,
                seqnum: 1,
            })
        }
    }

    pub fn send(&mut self) -> Result<()> {
        #[repr(C)]
        struct Nlmsg {
            hdr: libc::nlmsghdr,
            msg: rtnetlink::rtmsg,
        };

        let msg = Nlmsg {
            hdr: libc::nlmsghdr {
                nlmsg_len: size_of!(Nlmsg) as _,
                nlmsg_type: libc::RTM_GETROUTE as _,
                nlmsg_flags: (libc::NLM_F_REQUEST | libc::NLM_F_MULTI | libc::NLM_F_DUMP) as _,
                nlmsg_seq: self.seqnum,
                nlmsg_pid: self.address.nl_pid,
            },
            msg: rtnetlink::rtmsg {
                rtm_family: libc::AF_UNSPEC as _,
                rtm_dst_len: 0,
                rtm_src_len: 0,
                rtm_tos: 0,
                rtm_protocol: libc::RTPROT_UNSPEC as _,
                rtm_table: libc::RT_TABLE_LOCAL as _,
                rtm_scope: libc::RT_SCOPE_HOST as _,
                rtm_type: libc::RTN_LOCAL as _,
                rtm_flags: libc::RTM_F_NOTIFY as _,
            },
        };

        let status = unsafe {
            libc::sendto(
                self.fd.as_raw_fd(),
                &msg as *const _ as *const _,
                size_of!(Nlmsg) as _,
                libc::MSG_NOSIGNAL,
                &self.address as *const _ as *const _,
                std::mem::size_of_val(&self.address) as _,
            )
        };

        if status == size_of!(Nlmsg) as _ {
            self.seqnum += 1;
            Ok(())
        } else if status == -1 {
            Err(Error::IO(std::io::Error::last_os_error()))
        } else {
            unreachable!("datagram sockets do not send partial messages")
        }
    }

    pub fn recv(&self, buf: &mut Vec<u32>) -> Result<NetlinkIterator<'_>> {
        use std::mem::MaybeUninit;
        const RECVMSG_FLAGS: libc::c_int =
            libc::MSG_TRUNC | libc::MSG_CMSG_CLOEXEC | libc::MSG_DONTWAIT;
        let mut address = MaybeUninit::<libc::sockaddr_nl>::uninit();
        // These should be constants :(
        let cmsg_space = unsafe { libc::CMSG_SPACE(size_of!(libc::ucred) as _) } as usize;
        let cmsg_len = unsafe { libc::CMSG_LEN(size_of!(libc::ucred) as _) } as usize;
        union UcredCmsg {
            _dummy2: libc::cmsghdr,
            _data: [u8; CMSG_SPACE(size_of!(libc::ucred))],
        };
        let mut cmsghdr = MaybeUninit::<UcredCmsg>::uninit();
        let mut iovec = libc::iovec {
            iov_base: buf.as_mut_ptr() as *mut _,
            iov_len: buf.capacity() * size_of!(u32),
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
            let status =
                unsafe { libc::recvmsg(self.as_raw_fd(), &mut msghdr as _, RECVMSG_FLAGS) };
            if status < 0 {
                let errno = unsafe { *libc::__errno_location() };
                return Err(if errno == libc::ENOBUFS {
                    Error::Desync
                } else {
                    Error::IO(std::io::Error::from_raw_os_error(errno))
                });
            }
            if msghdr.msg_namelen as usize != size_of!(libc::sockaddr_nl)
                || msghdr.msg_controllen != cmsg_space
                || msghdr.msg_flags & (libc::MSG_TRUNC | libc::MSG_CTRUNC) != 0
            {
                return Err(Error::Desync);
            }
            // SAFETY: we just checked that the kernel filled in the right size
            // of address
            let address = unsafe { address.assume_init() };
            if address.nl_family != libc::AF_NETLINK as u16 {
                // wrong address family?
                return Err(Error::Desync);
            }
            if address.nl_pid != 0 {
                // message not from kernel
                continue;
            }
            let cmsghdr: libc::ucred = unsafe {
                let cmsghdr = libc::CMSG_FIRSTHDR(&msghdr);
                if cmsghdr.is_null() {
                    // kernel did not attach credentials
                    return Err(Error::Desync);
                }
                let cmsghdr = &*cmsghdr;
                if cmsghdr.cmsg_len != cmsg_len
                    || cmsghdr.cmsg_level != libc::SOL_SOCKET
                    || cmsghdr.cmsg_type != libc::SCM_CREDENTIALS
                {
                    // kernel did not attach credentials
                    return Err(Error::Desync);
                }
                std::ptr::read(libc::CMSG_DATA(cmsghdr) as _)
            };
            return if cmsghdr.pid == 0 && cmsghdr.uid == 0 && cmsghdr.gid == 0 {
                Ok(unsafe {
                    NetlinkIterator::new(core::slice::from_raw_parts(
                        iovec.iov_base as _,
                        status as _,
                    ))
                })
            } else {
                // kernel sent wrong credentials for its own message
                return Err(Error::Desync);
            };
        }
    }
}

impl AsRawFd for NetlinkSocket {
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn it_works() {
        const RTM_NEWROUTE: i32 = libc::RTM_NEWROUTE as _;
        const RTM_DELROUTE: i32 = libc::RTM_DELROUTE as _;
        let mut sock = NetlinkSocket::new().unwrap();
        sock.send().unwrap();
        unsafe {
            let mut s: std::mem::MaybeUninit<libc::sockaddr_nl> = std::mem::MaybeUninit::uninit();
            let mut l: libc::socklen_t = size_of!(libc::sockaddr_nl) as _;
            assert_eq!(
                libc::getsockname(sock.as_raw_fd(), s.as_mut_ptr() as _, &mut l),
                0
            );
            assert_eq!(l, size_of!(libc::sockaddr_nl) as libc::socklen_t);
            assert_eq!(size_of!(libc::sockaddr_nl), 12);
            assert_eq!(size_of!(libc::sa_family_t), 2);
            assert_eq!(std::ptr::read((s.as_ptr() as *const u16).offset(1)), 0);
            let s = s.assume_init();
            assert_eq!(s.nl_family, libc::AF_NETLINK as libc::sa_family_t);
            println!("Bound to PID {} and groups {}", s.nl_pid, s.nl_groups);
        }
        let mut buf = Vec::<u32>::with_capacity(8192);
        loop {
            let iter = sock.recv(&mut buf).unwrap();
            for (hdr, mut body) in iter {
                let flags = hdr.nlmsg_flags;
                if hdr.nlmsg_seq != 0 && hdr.nlmsg_seq != sock.seqnum - 1 {
                    continue;
                }
                match hdr.nlmsg_type as i32 {
                    libc::NLMSG_NOOP => continue,
                    libc::NLMSG_ERROR => panic!("we got an error!"),
                    libc::NLMSG_OVERRUN => panic!("buffer overrun!"),
                    libc::NLMSG_DONE => return,
                    RTM_NEWROUTE | RTM_DELROUTE => {
                        let mut the_ip = None;
                        let (hdr, iter) =
                            rtnetlink::read_rtmsg(&mut body).expect("bad message from kernel");
                        match (hdr.rtm_family as i32, hdr.rtm_table, hdr.rtm_type) {
                            (libc::AF_INET, libc::RT_TABLE_LOCAL, libc::RTN_LOCAL) => {}
                            (libc::AF_INET6, libc::RT_TABLE_LOCAL, libc::RTN_LOCAL) => {}
                            (libc::AF_INET, _, _) => continue,
                            (libc::AF_INET6, _, _) => continue,
                            _ => unreachable!(),
                        }
                        for i in iter.filter_map(|e| match e {
                            rtnetlink::RtaMessage::IPAddr(e) => Some(e),
                            rtnetlink::RtaMessage::Other => None,
                        }) {
                            assert!(std::mem::replace(&mut the_ip, Some(i.clone())).is_none());
                            assert_eq!(hdr.rtm_src_len, 0);
                            match i {
                                std::net::IpAddr::V4(_) => {
                                    assert_eq!(hdr.rtm_scope, libc::RT_SCOPE_HOST);
                                    assert_eq!(hdr.rtm_family as i32, libc::AF_INET);
                                }
                                std::net::IpAddr::V6(_) => {
                                    assert_eq!(hdr.rtm_scope, libc::RT_SCOPE_UNIVERSE);
                                    assert_eq!(hdr.rtm_family as i32, libc::AF_INET6);
                                }
                            }
                            println!("IP address {}/{}", i, hdr.rtm_dst_len)
                        }
                        assert!(the_ip.is_some());
                    }
                    _ => panic!("bad messge from kernel: type {:?}", hdr.nlmsg_type),
                }
                if flags & libc::NLM_F_MULTI as u16 == 0 {
                    break;
                }
            }
        }
    }
}
