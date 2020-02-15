#![allow(non_snake_case)]
#![forbid(clippy::all)]
use std::mem::size_of;
use std::ptr;

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
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

#[repr(C)]
struct rtattr {
    rta_len: u16,
    rta_type: u16,
}

const RTA_ALIGNTO: usize = 4;

pub struct RtaIterator<'a> {
    data: &'a [u32],
    len: usize,
}

pub enum RtaMessage {
    IPAddr(std::net::IpAddr),
    Other,
}

impl<'a> RtaIterator<'a> {
    pub fn new(data: super::netlink::NlMsgHeader<'a>) -> Option<(&'a rtmsg, Self)> {
        use std::mem::align_of;
        let length = data.header().nlmsg_len as usize - size_of::<libc::nlmsghdr>();
        let buffer = &data.data();
        assert_eq!(
            buffer.as_ptr() as usize - data.header() as *const _ as usize,
            16
        );
        if length < size_of::<rtmsg>() {
            return None;
        }
        // length must not be less than size_of::<rtmsg>() (12).
        if buffer.len() * size_of::<u32>() < length {
            return None;
        }
        // buffer.len() is at least 3
        let (start, data) = buffer.split_at(3);
        let _: [u8; 3 * size_of::<u32>()] = [0u8; size_of::<rtmsg>()];
        let _: [u8; align_of::<u32>()] = [0u8; align_of::<rtmsg>()];
        // SAFETY: size_of::<rtmsg>() == 3 * size_of::<u32>()
        //    and: align_of::<rtmsg>() == align_of::<u32>()
        //    and: `buffer.split_at()` panics if the buffer is shorter than 3.
        //    and: a `rtmsg` can be composed of any byte sequence
        //
        //    if either of the first two are false, the above two `let` statement are ill-typed.
        let msg: &'a rtmsg = unsafe { &*(start.as_ptr() as *const _) };
        Some((
            msg,
            Self {
                data,
                len: length - size_of::<rtmsg>(),
            },
        ))
    }
}

impl<'a> Iterator for RtaIterator<'a> {
    type Item = RtaMessage;
    fn next(&mut self) -> Option<RtaMessage> {
        use std::mem::align_of;
        if self.len < size_of::<rtattr>() {
            return None;
        }

        if self.data.len() * size_of::<u32>() < self.len {
            panic!("we always ensure that `data` has at least `len` bytes, but it is of length {} when len is {}; qed", self.data.len(), self.len)
        }

        let _: [u8; 0] = [0u8; (align_of::<u32>() < align_of::<rtattr>()) as usize];
        let _: [u8; 0] = [0u8; size_of::<u32>() - size_of::<rtattr>()];

        // SAFETY: size_of::<rtmsg>() == size_of::<u32>()
        //    and: align_of::<rtmsg>() < align_of::<u32>()
        //    and: rtmsg can have any bit pattern
        //    and: we checked that `data` points to at least `size_of::<rtattr>()` bytes
        let attr: &'a rtattr = unsafe { &*(self.data.as_ptr() as *const _) };
        let rta_len = attr.rta_len as usize;
        if rta_len < size_of::<rtattr>() || self.len < rta_len {
            return None;
        }

        let aligned_len = (rta_len + RTA_ALIGNTO - 1) / size_of::<u32>();
        // cannot panic because RTA_ALIGNTO == size_of::<u32>(), and so if `rta_len` does not
        // exceed the amount of data stored, neither will `aligned_len * size_of::<u32>()`.
        let (contents, new_data) = self.data[1..].split_at(aligned_len - 1);
        self.data = new_data;
        self.len -= aligned_len * size_of::<u32>();
        let (payload, ty) = (rta_len - RTA_ALIGNTO, attr.rta_type);
        Some(match (payload, ty) {
            (16, libc::RTA_DST) | (16, libc::RTA_SRC) => {
                assert_eq!(contents.len(), 4);
                let buffer: [u8; 16] = unsafe { ptr::read(contents.as_ptr() as _) };
                RtaMessage::IPAddr(buffer.into())
            }
            (4, libc::RTA_DST) | (4, libc::RTA_SRC) => {
                RtaMessage::IPAddr(contents[0].to_ne_bytes().into())
            }
            _ => RtaMessage::Other,
        })
    }
}
