use crate::{IfEvent, IpNet, Ipv4Net, Ipv6Net};
use fnv::FnvHashSet;
use futures::channel::mpsc::UnboundedReceiver;
use futures::future::Either;
use futures::stream::{Stream, TryStreamExt};
use rtnetlink::constants::{RTMGRP_IPV4_IFADDR, RTMGRP_IPV6_IFADDR};
use rtnetlink::packet::address::nlas::Nla;
use rtnetlink::packet::{AddressMessage, RtnlMessage};
use rtnetlink::proto::{Connection, NetlinkMessage, NetlinkPayload};
use rtnetlink::sys::{AsyncSocket, SmolSocket, SocketAddr};
use std::collections::VecDeque;
use std::future::Future;
use std::io::{Error, ErrorKind, Result};
use std::net::{Ipv4Addr, Ipv6Addr};
use std::ops::DerefMut;
use std::pin::Pin;
use std::task::{Context, Poll};

pub struct IfWatcher {
    conn: Connection<RtnlMessage, SmolSocket>,
    messages: UnboundedReceiver<(NetlinkMessage<RtnlMessage>, SocketAddr)>,
    addrs: FnvHashSet<IpNet>,
    queue: VecDeque<IfEvent>,
}

impl std::fmt::Debug for IfWatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("IfWatcher")
            .field("addrs", &self.addrs)
            .finish_non_exhaustive()
    }
}

impl IfWatcher {
    pub async fn new() -> Result<Self> {
        let (mut conn, handle, messages) = rtnetlink::new_connection_with_socket::<SmolSocket>()?;
        let groups = RTMGRP_IPV4_IFADDR | RTMGRP_IPV6_IFADDR;
        let addr = SocketAddr::new(0, groups);
        conn.socket_mut().socket_mut().bind(&addr)?;
        let mut stream = handle.address().get().execute();
        let mut addrs = FnvHashSet::default();
        let mut queue = VecDeque::default();

        loop {
            let fut = futures::future::select(conn, stream.try_next());
            match fut.await {
                Either::Left(_) => {
                    return Err(std::io::Error::new(
                        ErrorKind::BrokenPipe,
                        "rtnetlink socket closed",
                    ))
                }
                Either::Right((x, c)) => {
                    conn = c;
                    match x {
                        Ok(Some(msg)) => {
                            for net in iter_nets(msg) {
                                if addrs.insert(net) {
                                    queue.push_back(IfEvent::Up(net));
                                }
                            }
                        }
                        Ok(None) => break,
                        Err(err) => return Err(Error::new(ErrorKind::Other, err)),
                    }
                }
            }
        }
        Ok(Self {
            conn,
            messages,
            addrs,
            queue,
        })
    }

    pub fn iter(&self) -> impl Iterator<Item = &IpNet> {
        self.addrs.iter()
    }

    fn add_address(&mut self, msg: AddressMessage) {
        for net in iter_nets(msg) {
            if self.addrs.insert(net) {
                self.queue.push_back(IfEvent::Up(net));
            }
        }
    }

    fn rem_address(&mut self, msg: AddressMessage) {
        for net in iter_nets(msg) {
            if self.addrs.remove(&net) {
                self.queue.push_back(IfEvent::Down(net));
            }
        }
    }
}

impl Future for IfWatcher {
    type Output = Result<IfEvent>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        log::trace!("polling IfWatcher {:p}", self.deref_mut());
        if Pin::new(&mut self.conn).poll(cx).is_ready() {
            return Poll::Ready(Err(std::io::Error::new(
                ErrorKind::BrokenPipe,
                "rtnetlink socket closed",
            )));
        }
        while let Poll::Ready(Some((message, _))) = Pin::new(&mut self.messages).poll_next(cx) {
            match message.payload {
                NetlinkPayload::Error(err) => return Poll::Ready(Err(err.to_io())),
                NetlinkPayload::InnerMessage(msg) => match msg {
                    RtnlMessage::NewAddress(msg) => self.add_address(msg),
                    RtnlMessage::DelAddress(msg) => self.rem_address(msg),
                    _ => {}
                },
                _ => {}
            }
        }
        if let Some(event) = self.queue.pop_front() {
            return Poll::Ready(Ok(event));
        }
        Poll::Pending
    }
}

fn iter_nets(msg: AddressMessage) -> impl Iterator<Item = IpNet> {
    let prefix = msg.header.prefix_len;
    let family = msg.header.family;
    msg.nlas.into_iter().filter_map(move |nla| {
        if let Nla::Address(octets) = nla {
            match family {
                2 => {
                    let mut addr = [0; 4];
                    addr.copy_from_slice(&octets);
                    Some(IpNet::V4(
                        Ipv4Net::new(Ipv4Addr::from(addr), prefix).unwrap(),
                    ))
                }
                10 => {
                    let mut addr = [0; 16];
                    addr.copy_from_slice(&octets);
                    Some(IpNet::V6(
                        Ipv6Net::new(Ipv6Addr::from(addr), prefix).unwrap(),
                    ))
                }
                _ => None,
            }
        } else {
            None
        }
    })
}
