use crate::unix::{aligned_buffer::U32AlignedBuffer, Fd};
use crate::IfEvent;
use async_io::Async;
use ipnet::{IpNet, Ipv4Net, Ipv6Net};
use netlink::NetlinkIterator;
use std::{
    collections::VecDeque,
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

const RTMGRP_IPV4_IFADDR: u32 = 0x10;
const RTMGRP_IPV6_IFADDR: u32 = 0x100;
const RTM_NEWADDR: i32 = libc::RTM_NEWADDR as _;
const RTM_DELADDR: i32 = libc::RTM_DELADDR as _;

impl NetlinkSocket {
    pub fn new() -> Result<Self> {
        unsafe {
            let flags = libc::SOCK_RAW | libc::SOCK_CLOEXEC;
            let fd = Fd::new(errno!(libc::socket(
                libc::AF_NETLINK,
                flags,
                libc::NETLINK_ROUTE
            ))?)?;
            let mut address: sockaddr_nl = std::mem::zeroed();
            address.nl_family = libc::AF_NETLINK as _;
            address.nl_groups = RTMGRP_IPV4_IFADDR | RTMGRP_IPV6_IFADDR;
            let ptr = &mut address as *mut _;
            let mut size = size_of!(sockaddr_nl) as u32;
            errno!(libc::bind(fd.as_raw_fd(), ptr as *mut _, size)).unwrap();
            errno!(libc::getsockname(fd.as_raw_fd(), ptr as *mut _, &mut size)).unwrap();
            let pid = address.nl_pid;
            address.nl_pid = 0;
            address.nl_groups = 0;
            Ok(Self {
                fd,
                address,
                seqnum: 0,
                pid,
            })
        }
    }

    pub async fn send_getaddr(&mut self) -> Result<()> {
        #[repr(C)]
        struct Nlmsg {
            hdr: libc::nlmsghdr,
            msg: rtnetlink::rtmsg,
            //family: u8,
        };
        if self.seqnum == u32::max_value() {
            self.seqnum = 1;
        } else {
            self.seqnum += 1;
        }
        let msg = Nlmsg {
            hdr: libc::nlmsghdr {
                nlmsg_len: size_of!(Nlmsg) as _,
                nlmsg_type: libc::RTM_GETADDR,
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
            }, //family: libc::AF_UNSPEC as _,
        };
        self.fd
            .write_with(|fd| unsafe {
                let msg: *const _ = &msg;
                let address: *const _ = &self.address;
                errno!(libc::sendto(
                    fd.as_raw_fd(),
                    msg as *const _,
                    size_of!(Nlmsg),
                    libc::MSG_NOSIGNAL,
                    address as _,
                    std::mem::size_of_val(&self.address) as _,
                ))
                .unwrap();
                Ok(())
            })
            .await
    }

    pub async fn recv_event(&mut self, queue: &mut VecDeque<IfEvent>) -> Result<()> {
        let mut buf = [0u64; 512];
        let iter = loop {
            let res = self
                .fd
                .read_with(|fd| unsafe {
                    let mut address = MaybeUninit::<libc::sockaddr_nl>::uninit();
                    let mut iovec = libc::iovec {
                        iov_base: buf.as_mut_ptr() as *mut _,
                        iov_len: buf.len() * size_of!(u64),
                    };
                    let mut msghdr = libc::msghdr {
                        msg_name: address.as_mut_ptr() as _,
                        msg_namelen: size_of!(libc::sockaddr_nl) as _,
                        msg_iov: &mut iovec,
                        msg_iovlen: 1,
                        msg_control: std::ptr::null_mut(),
                        msg_controllen: 0,
                        msg_flags: 0,
                    };

                    let flags = libc::MSG_TRUNC | libc::MSG_CMSG_CLOEXEC;
                    let status = errno!(libc::recvmsg(fd.as_raw_fd(), &mut msghdr, flags))?;

                    if msghdr.msg_namelen as usize != size_of!(libc::sockaddr_nl)
                        || msghdr.msg_flags & (libc::MSG_TRUNC | libc::MSG_CTRUNC) != 0
                    {
                        log::error!("rtnetlink message was truncated");
                        return Ok(None);
                    }
                    // SAFETY: we just checked that the kernel filled in the right size
                    // of address
                    let address = address.assume_init();
                    if address.nl_family != libc::AF_NETLINK as u16 {
                        log::trace!("wrong address family");
                        return Ok(None);
                    }
                    if address.nl_pid != 0 {
                        log::trace!("message not from kernel");
                        return Ok(None);
                    }
                    let msg =
                        std::slice::from_raw_parts(iovec.iov_base as *const u8, status as usize);
                    Ok(Some(msg))
                })
                .await?;
            if let Some(msg) = res {
                break NetlinkIterator::new(msg);
            }
        };
        for (hdr, mut body) in iter {
            match hdr.nlmsg_type as i32 {
                libc::NLMSG_NOOP => {}
                libc::NLMSG_DONE => return Ok(()),
                RTM_NEWADDR | RTM_DELADDR => {
                    read_ifaddrmsg(queue, hdr.nlmsg_type as i32, &mut body);
                }
                other => {
                    println!("{}", other);
                }
            }
        }
        Ok(())
    }
}

fn read_ifaddrmsg<'a>(queue: &mut VecDeque<IfEvent>, ty: i32, msg: &mut U32AlignedBuffer<'a>) {
    let (hdr, iter) = rtnetlink::read_msg::<rtnetlink::ifaddrmsg>(msg)
        .expect("kernel only sends valid messages; qed");
    let family = hdr.ifa_family as i32;
    if family != libc::AF_INET && family != libc::AF_INET6 {
        log::trace!("Skipping unknown address family");
        return;
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
    let prefix = hdr.ifa_prefixlen;
    let ipnet = match ip {
        IpAddr::V4(ip) => IpNet::V4(Ipv4Net::new(ip, prefix).expect("kernel sent valid prefix")),
        IpAddr::V6(ip) => IpNet::V6(Ipv6Net::new(ip, prefix).expect("kernel sent valid prefix")),
    };
    match ty {
        RTM_NEWADDR => queue.push_back(IfEvent::Up(ipnet)),
        RTM_DELADDR => queue.push_back(IfEvent::Down(ipnet)),
        _ => {}
    }
}

impl AsRawFd for NetlinkSocket {
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}
