#![forbid(clippy::all)]
use super::super::aligned_buffer::{FromBuffer, U32AlignedBuffer};

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
#[allow(non_camel_case_types)]
pub(crate) struct ifaddrmsg {
    pub(crate) ifa_family: u8,
    pub(crate) ifa_prefixlen: u8,
    pub(crate) ifa_flags: u8,
    pub(crate) ifa_scope: u8,
    pub(crate) ifa_index: u32,
}

// SAFETY: rtmsg can have any bit pattern
unsafe impl FromBuffer for ifaddrmsg {
    fn len(&self, size: usize) -> u32 {
        size as _
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
#[allow(non_camel_case_types)]
pub(crate) struct rtmsg {
    pub(crate) rtm_family: u8,
    pub(crate) rtm_dst_len: u8,
    pub(crate) rtm_src_len: u8,
    pub(crate) rtm_tos: u8,
    pub(crate) rtm_table: u8,
    pub(crate) rtm_protocol: u8,
    pub(crate) rtm_scope: u8,
    pub(crate) rtm_type: u8,
    pub(crate) rtm_flags: u32,
}

// SAFETY: rtmsg can have any bit pattern
unsafe impl FromBuffer for rtmsg {
    fn len(&self, size: usize) -> u32 {
        size as _
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
#[allow(non_camel_case_types)]
struct rtattr {
    rta_len: u16,
    rta_type: u16,
}

// SAFETY: rtattr can have any bit pattern
unsafe impl FromBuffer for rtattr {
    fn len(&self, _: usize) -> u32 {
        self.rta_len.into()
    }
}

pub(crate) struct RtaIterator<'a>(U32AlignedBuffer<'a>);

pub(crate) fn read_msg<'a, M: FromBuffer>(
    buffer: &mut U32AlignedBuffer<'a>,
) -> Option<(M, RtaIterator<'a>)> {
    let (msg, new_buffer) = buffer.read()?;
    let iterator = RtaIterator(new_buffer);
    Some((msg, iterator))
}

pub(crate) enum RtaMessage {
    IPAddr(std::net::IpAddr),
    Other,
}

impl Iterator for RtaIterator<'_> {
    type Item = RtaMessage;

    fn next(&mut self) -> Option<RtaMessage> {
        use core::convert::TryInto;
        let (attr, buf): (rtattr, _) = self.0.read()?;
        Some(match attr.rta_type {
            libc::RTA_DST => match buf.try_into().ok() {
                Some(e) => RtaMessage::IPAddr(e),
                None => RtaMessage::Other,
            },
            other => {
                println!("other {}", other);
                RtaMessage::Other
            }
        })
    }
}
