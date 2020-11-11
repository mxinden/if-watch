use crate::unix::{Fd, Status};
use crate::IfEvent;
use async_io::Async;
use ipnet::{IpNet, Ipv4Net, Ipv6Net};
use netlink::NetlinkIterator;
use std::{
    collections::{HashSet, VecDeque},
    io::{Error, Result},
    mem::MaybeUninit,
    net::IpAddr,
    os::unix::prelude::*,
};

mod netlink;
mod rtnetlink;

#[allow(non_camel_case_types)]
#[derive(Debug)]
#[repr(C)]
struct sockaddr_nl {
    nl_family: libc::sa_family_t,
    nl_pad: libc::c_ushort,
    nl_pid: u32,
    nl_groups: u32,
}

#[derive(Debug)]
pub struct NetlinkSocket {
    fd: Async<Fd>,
    address: sockaddr_nl,
    seqnum: u32,
    pid: u32,
}

const RTMGRP_IPV4_ROUTE: u32 = 0x40;
const RTMGRP_IPV6_ROUTE: u32 = 0x400;
const RTM_NEWROUTE: i32 = libc::RTM_NEWROUTE as _;
const RTM_DELROUTE: i32 = libc::RTM_DELROUTE as _;
const SOCKADDR_NL_SIZE: libc::socklen_t = size_of!(sockaddr_nl) as _;

impl NetlinkSocket {
    pub fn new() -> Result<Self> {
        let fd = unsafe {
            libc::socket(
                libc::PF_NETLINK,
                libc::SOCK_RAW | libc::SOCK_CLOEXEC,
                libc::NETLINK_ROUTE,
            )
        };
        if fd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        let fd = Fd::new(fd)?;
        unsafe {
            let _: [u8; SOCKADDR_NL_SIZE as usize - size_of!(libc::sockaddr_nl)] = [];
            let mut address: sockaddr_nl = std::mem::zeroed();
            address.nl_family = libc::AF_NETLINK as _;
            address.nl_groups = RTMGRP_IPV4_ROUTE | RTMGRP_IPV6_ROUTE;
            let ptr: *mut _ = &mut address;
            let mut addrlen = SOCKADDR_NL_SIZE;
            let bind_result = libc::bind(fd.as_raw_fd(), ptr as _, addrlen);
            if bind_result < 0 {
                return Err(Error::last_os_error());
            }
            while libc::getsockname(fd.as_raw_fd(), ptr as _, &mut addrlen) < 0 {
                addrlen = SOCKADDR_NL_SIZE
            }
            let pid = address.nl_pid;
            address.nl_pid = 0;
            address.nl_groups = 0;
            assert_eq!(addrlen, SOCKADDR_NL_SIZE);
            Ok(Self {
                fd,
                address,
                seqnum: 1,
                pid,
            })
        }
    }

    pub async fn send(&mut self) -> Result<()> {
        #[repr(C)]
        struct Nlmsg {
            hdr: libc::nlmsghdr,
            msg: rtnetlink::rtmsg,
        };
        if self.seqnum == u32::max_value() {
            self.seqnum = 1;
        } else {
            self.seqnum += 1;
        }
        let msg = Nlmsg {
            hdr: libc::nlmsghdr {
                nlmsg_len: size_of!(Nlmsg) as _,
                nlmsg_type: libc::RTM_GETROUTE,
                nlmsg_flags: (libc::NLM_F_REQUEST | libc::NLM_F_MULTI | libc::NLM_F_DUMP) as _,
                nlmsg_seq: self.seqnum,
                nlmsg_pid: self.address.nl_pid,
            },
            msg: rtnetlink::rtmsg {
                rtm_family: libc::AF_UNSPEC as _,
                rtm_dst_len: 0,
                rtm_src_len: 0,
                rtm_tos: 0,
                rtm_protocol: libc::RTPROT_UNSPEC,
                rtm_table: libc::RT_TABLE_LOCAL,
                rtm_scope: libc::RT_SCOPE_HOST,
                rtm_type: libc::RTN_LOCAL,
                rtm_flags: libc::RTM_F_NOTIFY,
            },
        };

        self.fd
            .write_with(|fd| {
                let msg: *const _ = &msg;
                let address: *const _ = &self.address;
                let status = unsafe {
                    libc::sendto(
                        fd.as_raw_fd(),
                        msg as *const _,
                        size_of!(Nlmsg),
                        libc::MSG_NOSIGNAL,
                        address as _,
                        std::mem::size_of_val(&self.address) as _,
                    )
                };

                if status == size_of!(Nlmsg) as _ {
                    Ok(())
                } else if status == -1 {
                    Err(std::io::Error::last_os_error())
                } else {
                    unreachable!("datagram sockets do not send partial messages")
                }
            })
            .await
    }

    async fn recv(&self, buf: &mut Vec<u64>, flags: libc::c_int) -> Status<NetlinkIterator<'_>> {
        loop {
            let res = self
                .fd
                .read_with(|fd| {
                    let mut address = MaybeUninit::<libc::sockaddr_nl>::uninit();
                    let mut iovec = libc::iovec {
                        iov_base: buf.as_mut_ptr() as *mut _,
                        iov_len: buf.capacity() * size_of!(u64),
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

                    let status = unsafe { libc::recvmsg(fd.as_raw_fd(), &mut msghdr, flags) };
                    if status < 0 {
                        return Ok(Some(match unsafe { *libc::__errno_location() } {
                            libc::ENOBUFS => Status::Desync,
                            libc::EAGAIN => {
                                return Err(std::io::Error::from_raw_os_error(libc::EAGAIN))
                            }
                            errno => Status::IO(std::io::Error::from_raw_os_error(errno)),
                        }));
                    }

                    if msghdr.msg_namelen as usize != size_of!(libc::sockaddr_nl)
                        || msghdr.msg_flags & (libc::MSG_TRUNC | libc::MSG_CTRUNC) != 0
                    {
                        return Ok(Some(Status::Desync));
                    }
                    // SAFETY: we just checked that the kernel filled in the right size
                    // of address
                    let address = unsafe { address.assume_init() };
                    if address.nl_family != libc::AF_NETLINK as u16 {
                        // wrong address family?
                        return Ok(Some(Status::Desync));
                    }
                    if address.nl_pid != 0 {
                        // message not from kernel
                        return Ok(None);
                    }
                    Ok(Some(Status::Data(unsafe {
                        NetlinkIterator::new(core::slice::from_raw_parts(
                            iovec.iov_base as _,
                            status as _,
                        ))
                    })))
                })
                .await;
            if let Ok(Some(status)) = res {
                return status;
            }
        }
    }

    pub(super) async fn next(
        &mut self,
        buf: &mut Vec<u64>,
        queue: &mut VecDeque<IfEvent>,
        hash: &mut HashSet<IpNet>,
    ) -> Status<()> {
        let flags = libc::MSG_TRUNC | libc::MSG_CMSG_CLOEXEC;
        match self.recv(buf, flags).await {
            Status::Data(iter) => {
                dump_iterator(queue, 0, iter, hash, self.pid);
                Status::Data(())
            }
            Status::Desync => Status::Desync,
            Status::IO(e) => Status::IO(e),
        }
    }

    pub async fn resync(
        &mut self,
        buf: &mut Vec<u64>,
        queue: &mut VecDeque<IfEvent>,
        hash: &mut HashSet<IpNet>,
    ) -> Result<()> {
        let flags = libc::MSG_TRUNC | libc::MSG_CMSG_CLOEXEC;
        self.send().await?;
        loop {
            match self.recv(buf, flags).await {
                Status::IO(e) => return Err(e),
                Status::Desync => {
                    queue.clear();
                    continue;
                }
                Status::Data(iter) => {
                    if dump_iterator(queue, self.seqnum, iter, hash, self.pid) {
                        return Ok(());
                    }
                }
            }
        }
    }
}

fn dump_iterator(
    queue: &mut VecDeque<IfEvent>,
    seqnum: u32,
    iter: NetlinkIterator<'_>,
    hash: &mut HashSet<IpNet>,
    pid: u32,
) -> bool {
    for (hdr, mut body) in iter {
        if hdr.nlmsg_pid != 0 && hdr.nlmsg_pid != pid {
            // println!("Rejecting message from wrong process {}", hdr.nlmsg_pid);
            continue;
        }
        if hdr.nlmsg_seq != seqnum {
            // println!("Got bogus sequence number {}", hdr.nlmsg_seq);
            continue;
        }
        match hdr.nlmsg_type as i32 {
            libc::NLMSG_NOOP => {}
            libc::NLMSG_DONE => return true,
            msg @ RTM_NEWROUTE | msg @ RTM_DELROUTE => {
                let (hdr, iter) = rtnetlink::read_rtmsg(&mut body)
                    .expect("kernel only sends valid messages; qed");
                match (hdr.rtm_family as i32, hdr.rtm_table, hdr.rtm_type) {
                    (libc::AF_INET, libc::RT_TABLE_LOCAL, libc::RTN_LOCAL) => {}
                    (libc::AF_INET6, libc::RT_TABLE_LOCAL, libc::RTN_LOCAL) => {}
                    _ => continue,
                }
                let ip = iter
                    .filter_map(|e| match e {
                        rtnetlink::RtaMessage::IPAddr(e) => Some(e),
                        rtnetlink::RtaMessage::Other => None,
                    })
                    .next()
                    .expect(
                        "RTM_NEWROUTE and RTM_DELROUTE messages always contain a destination \
                         address, so if you hit this panic, your kernel is broken; qed",
                    );
                let prefix = hdr.rtm_dst_len;
                let ipnet = match ip {
                    IpAddr::V4(ip) => {
                        IpNet::V4(Ipv4Net::new(ip, prefix).expect("kernel sent valid prefix"))
                    }
                    IpAddr::V6(ip) => {
                        IpNet::V6(Ipv6Net::new(ip, prefix).expect("kernel sent valid prefix"))
                    }
                };
                match msg {
                    RTM_NEWROUTE if hash.insert(ipnet) => queue.push_back(IfEvent::Up(ipnet)),
                    RTM_DELROUTE if hash.remove(&ipnet) => queue.push_back(IfEvent::Down(ipnet)),
                    _ => {}
                }
            }
            _ => {}
        }
    }
    false
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
        async_io::block_on(async {
            let mut sock = NetlinkSocket::new().unwrap();
            let flags = libc::MSG_TRUNC | libc::MSG_CMSG_CLOEXEC | libc::MSG_DONTWAIT;

            sock.send().await.unwrap();
            unsafe {
                let mut s: MaybeUninit<libc::sockaddr_nl> = std::mem::MaybeUninit::uninit();
                std::ptr::write((s.as_mut_ptr() as *mut u16).offset(1), 0xFFFFu16);
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
                // println!("Bound to PID {} and groups {}", s.nl_pid,
                // s.nl_groups);
            }
            let mut buf = Vec::with_capacity(8192);
            loop {
                let iter = match sock.recv(&mut buf, flags).await {
                    Status::Data(e) => e,
                    _ => panic!(),
                };
                for (hdr, mut body) in iter {
                    let flags = hdr.nlmsg_flags;
                    if hdr.nlmsg_seq != 0 && hdr.nlmsg_seq != sock.seqnum {
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
                                (libc::AF_INET, ..) => continue,
                                (libc::AF_INET6, ..) => continue,
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
                                // println!("IP address {}/{}", i,
                                // hdr.rtm_dst_len)
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
        });
    }
}
