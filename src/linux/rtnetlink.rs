#![allow(non_snake_case)]
#![forbid(clippy::all)]
use super::sys::rtnetlink;
use std::mem::size_of;
use std::ptr;

macro_rules! const_assert_eq {
    ($a: expr, $b: expr) => {
        #[allow(unreachable_code)]
        (if false {
            let _: [u8; $a] = [0u8; $b];
        })
    };
}

macro_rules! assert_same_size {
    ($a: ty, $b: ty) => {
        const_assert_eq!(size_of::<$a>(), size_of::<$b>())
    };
}

const RTA_ALIGNTO: usize = rtnetlink::RTA_ALIGNTO as _;

// This is guaranteed to never overflow, since `i32` can hold `u16::MAX + RTA_ALIGNTO`.
const fn RTA_ALIGN(len: u16) -> i32 {
    (((len as usize) + RTA_ALIGNTO - 1) & !(RTA_ALIGNTO - 1)) as i32
}

// Safety: rta must be 4-byte aligned and point to `(len + 3) & !3` bytes of valid memory.
unsafe fn RTA_OK(rta: *const rtnetlink::rtattr, len: i32) -> bool {
    assert_same_size!(u16, libc::c_ushort);
    assert_same_size!(rtnetlink::rtattr, u32);
    assert_same_size!(rtnetlink::rtattr, [libc::c_ushort; 2]);
    if len < size_of::<rtnetlink::rtattr>() as i32 {
        return false;
    }
    let len_found = ptr::read(rta as *const u16);
    size_of::<rtnetlink::rtattr>() <= len_found.into() && len >= len_found.into()
}

// Safety: rta must be 4-byte aligned and point to `(len + 3) & !3` bytes of valid memory.
unsafe fn RTA_NEXT(rta: *const rtnetlink::rtattr, len: &mut i32) -> *const rtnetlink::rtattr {
    const_assert_eq!(std::mem::align_of::<rtnetlink::rtattr>(), 2);
    let aligned_len = RTA_ALIGN(ptr::read(rta as *const u16));
    *len -= aligned_len as i32;
    (rta as *const u8).wrapping_add(aligned_len as usize) as *const _
}

fn RTA_DATA(rta: *const rtnetlink::rtattr) -> *const u32 {
    rta.wrapping_add(1) as *const _
}

pub struct RtaIterator<'a> {
    phantom: std::marker::PhantomData<&'a [u32]>,
    pointer: *const rtnetlink::rtattr,
    len: i32,
}

pub enum RtaMessage {
    IPv6Addr([u32; 4]),
    IPv4Addr(u32),
    Other,
}

impl<'a> Iterator for RtaIterator<'a> {
    type Item = RtaMessage;
    fn next(&mut self) -> Option<RtaMessage> {
        Some(unsafe {
            if !RTA_OK(self.pointer, self.len) {
                return None;
            }
            let current_pointer = self.pointer;
            self.pointer = RTA_NEXT(current_pointer, &mut self.len);
            let payload = ptr::read(current_pointer as *const u16) as i32
                - RTA_ALIGN(size_of::<rtnetlink::rtattr>() as _);
            let ty = ptr::read((current_pointer as *const u16).wrapping_add(1));
            match (payload, ty) {
                (16, libc::RTA_DST) | (16, libc::RTA_SRC) => {
                    RtaMessage::IPv6Addr(ptr::read(RTA_DATA(current_pointer) as *const _))
                }
                (4, libc::RTA_DST) | (4, libc::RTA_SRC) => {
                    RtaMessage::IPv4Addr(ptr::read(RTA_DATA(current_pointer) as *const _))
                }
                _ => RtaMessage::Other,
            }
        })
    }
}
