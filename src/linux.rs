use std::{
    io,
    os::unix::io::{AsRawFd, RawFd},
};
mod netlink;
mod rtnetlink;
pub use netlink::NetlinkIterator;
pub use rtnetlink::RtaIterator;

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
            msg: rtnetlink::rtmsg,
        };

        let _: [u8; size_of!(Nlmsg)] = [0u8; NETLINK_SIZE + RTMSG_SIZE];
        let _: [u8; SPACE] = [0u8; NETLINK_SIZE + RTMSG_SIZE];

        let msg = Nlmsg {
            hdr: libc::nlmsghdr {
                nlmsg_len: SPACE as _,
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
        use std::mem::MaybeUninit;
        let mut address = MaybeUninit::<libc::sockaddr_nl>::uninit();
        union UcredCmsg {
            _dummy2: libc::cmsghdr,
            _data: [u8; CMSG_SPACE(size_of!(libc::ucred))],
        };
        let _: [u8; 0] = [0u8; align_of!(UcredCmsg) - align_of!(libc::cmsghdr)];
        let mut cmsghdr = MaybeUninit::<UcredCmsg>::zeroed();
        let mut iovec = libc::iovec {
            iov_base: buf.as_mut_ptr() as *mut _,
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
            if msghdr.msg_namelen != size_of!(libc::sockaddr_nl) as u32
                || msghdr.msg_controllen
                    != unsafe { libc::CMSG_SPACE(size_of!(libc::ucred) as _) } as _
            {
                break Err(std::io::ErrorKind::InvalidData.into());
            }
            let address = unsafe { address.assume_init() };
            if address.nl_family != libc::AF_NETLINK as u16 {
                continue;
            }
            let cmsghdr = unsafe {
                let pointer = libc::CMSG_FIRSTHDR(&msghdr);
                if pointer.is_null() {
                    // kernel did not attach credentials
                    break Err(std::io::ErrorKind::InvalidData.into());
                }
                &*pointer
            };
            if cmsghdr.cmsg_len != unsafe { libc::CMSG_LEN(size_of!(libc::ucred) as _) } as _
                || cmsghdr.cmsg_level != libc::SOL_SOCKET
                || cmsghdr.cmsg_type != libc::SCM_CREDENTIALS
            {
                // kernel did not attach credentials
                break Err(std::io::ErrorKind::InvalidData.into());
            }
            let cmsghdr = unsafe { &*(libc::CMSG_DATA(cmsghdr) as *const libc::ucred) };
            if address.nl_pid == 0 && cmsghdr.pid == 0 && cmsghdr.uid == 0 && cmsghdr.gid == 0 {
                break Ok(status as usize);
            } /* else: ignore packet not from kernel */
        }
    }
}

const RTMSG_SIZE: usize = size_of!(rtnetlink::rtmsg);

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
        const RTM_NEWROUTE: i32 = libc::RTM_NEWROUTE as _;
        const RTM_DELROUTE: i32 = libc::RTM_DELROUTE as _;
        let mut sock = NetlinkSocket::new().unwrap();
        sock.send().unwrap();
        let buf = &mut [0u32; 8192];
        loop {
            let len = match sock.recv(buf) {
                Ok(len) => len,
                Err(e) => {
                    assert_eq!(e.kind(), std::io::ErrorKind::WouldBlock);
                    break;
                }
            };
            for i in NetlinkIterator::new(buf, len) {
                let hdr = i.header();
                match hdr.nlmsg_type as i32 {
                    libc::NLMSG_NOOP => continue,
                    libc::NLMSG_ERROR => panic!("we got an error!"),
                    libc::NLMSG_OVERRUN => panic!("buffer overrun!"),
                    libc::NLMSG_DONE => return,
                    RTM_NEWROUTE | RTM_DELROUTE => {
                        let mut the_ip = None;
                        let (hdr, iter) = RtaIterator::new(i).expect("bad message from kernel");
                        match (hdr.rtm_family as i32, hdr.rtm_table, hdr.rtm_type) {
                            (libc::AF_INET, libc::RT_TABLE_LOCAL, libc::RTN_LOCAL) => {}
                            (libc::AF_INET6, libc::RT_TABLE_LOCAL, libc::RTN_LOCAL) => {}
                            _ => continue,
                        }
                        for i in iter.filter_map(|e| match e {
                            rtnetlink::RtaMessage::IPAddr(e) => Some(e),
                            _ => None,
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
                    _ => panic!("bad messge from kernel"),
                }
            }
        }
    }
}
