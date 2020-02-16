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
        let mut address = std::mem::MaybeUninit::<libc::sockaddr_nl>::uninit();

        let mut iovec = libc::iovec {
            iov_base: buf.as_mut_ptr() as *mut std::ffi::c_void,
            iov_len: buf.len() * size_of!(u32),
        };

        let mut msghdr = libc::msghdr {
            msg_name: address.as_mut_ptr() as _,
            msg_namelen: size_of!(libc::sockaddr_nl) as u32,
            msg_iov: &mut iovec,
            msg_iovlen: 1,
            msg_control: std::ptr::null_mut(),
            msg_controllen: 0,
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
            if msghdr.msg_namelen != size_of!(libc::sockaddr_nl) as u32 {
                break Err(std::io::ErrorKind::InvalidData.into());
            }
            let address = unsafe { address.assume_init() };
            if libc::AF_NETLINK != address.nl_family.into() {
                unreachable!("this is a netlink socket, so the address family is AF_NETLINK; qed");
            }
            if address.nl_pid == 0 {
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
            let len = sock.recv(buf).unwrap();
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
