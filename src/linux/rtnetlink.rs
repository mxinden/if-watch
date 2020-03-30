#![forbid(clippy::all)]
use crate::aligned_buffer::{FromBuffer, U32AlignedBuffer};

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
#[allow(non_camel_case_types)]
pub struct rtmsg {
    pub rtm_family: u8,
    pub rtm_dst_len: u8,
    pub rtm_src_len: u8,
    pub rtm_tos: u8,
    pub rtm_table: u8,
    pub rtm_protocol: u8,
    pub rtm_scope: u8,
    pub rtm_type: u8,
    pub rtm_flags: u32,
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

pub struct RtaIterator<'a>(U32AlignedBuffer<'a>);

pub fn read_rtmsg<'a>(buffer: &mut U32AlignedBuffer<'a>) -> Option<(rtmsg, RtaIterator<'a>)> {
    let (rtmsg, new_buffer) = buffer.read()?;
    let iterator = RtaIterator(new_buffer);
    Some((rtmsg, iterator))
}

pub enum RtaMessage {
    IPAddr(std::net::IpAddr),
    Other,
}

impl<'a> Iterator for RtaIterator<'a> {
    type Item = RtaMessage;
    fn next(&mut self) -> Option<RtaMessage> {
        use core::convert::TryInto;
        let (attr, buf): (rtattr, _) = self.0.read()?;
        Some(match attr.rta_type as _ {
            libc::RTA_DST | libc::RTA_SRC => match buf.try_into().ok() {
                Some(e) => RtaMessage::IPAddr(e),
                None => RtaMessage::Other,
            },
            _ => RtaMessage::Other,
        })
    }
}
