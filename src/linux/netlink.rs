#![allow(non_snake_case)]
use std::marker::PhantomData;
use super::sys::netlink;

#[derive(Debug)]
pub struct NetlinkIterator<'a> {
    phantom: PhantomData<&'a [u32]>,
    len: usize,
    pointer: *const netlink::nlmsghdr,
}

const U32_SIZE: usize = std::mem::size_of::<u32>();
pub const NLMSG_HDRLEN: usize = NLMSG_ALIGN(std::mem::size_of::<netlink::nlmsghdr>());
// SAFETY: nln is 4-byte aligned and points to at least `len` bytes of valid memory.
// If `len` is zero, `nln` is ignored
#[inline]
unsafe fn NLMSG_OK(nln: *const netlink::nlmsghdr, len: usize) -> bool {
    if false {
        let _: [u8; 0] = [0u8; (U32_SIZE > std::mem::size_of::<usize>()) as usize];
    }
    len >= NETLINK_SIZE
        && (*nln).nlmsg_len as usize >= NETLINK_SIZE
        && (*nln).nlmsg_len as usize <= len
        && NLMSG_ALIGN((*nln).nlmsg_len as usize) as usize <= len
}

#[inline]
const fn NLMSG_ALIGN(len: usize) -> usize {
    (len + NETLINK_ALIGNMENT - 1) & !(NETLINK_ALIGNMENT - 1)
}

// SAFETY: nlh is 4-byte aligned and points to `len` bytes of valid memory, AND
// `NLMSG_OK(nlh, len)` holds.
unsafe fn NLMSG_NEXT(
    nlh: *const netlink::nlmsghdr,
    len: &mut usize,
) -> *const netlink::nlmsghdr {
    let length = NLMSG_ALIGN((*nlh).nlmsg_len as _);
    *len -= length;
    // must cast, otherwise `len` is multiplied by an implicit `size_of::<nlmsghdr>()`!
    (nlh as usize + length) as *const _
}

#[inline]
pub(crate) const fn NLMSG_LENGTH(size: usize) -> usize {
    size + NLMSG_HDRLEN
}

pub(crate) const fn NLMSG_SPACE(size: usize) -> usize {
    NLMSG_ALIGN(NLMSG_LENGTH(size))
}

#[derive(Debug)]
pub struct NlMsgHeader<'a> {
    pointer: &'a netlink::nlmsghdr,
}

impl<'a> NlMsgHeader<'a> {
    pub fn data(&self) -> *const libc::c_void {
        (self.pointer as *const _ as *const u8).wrapping_add(NLMSG_LENGTH(0)) as _
    }
    pub fn inner(&self) -> &netlink::nlmsghdr {
        self.pointer
    }
}

impl<'a> NetlinkIterator<'a> {
    pub fn new(buffer: &'a [u32], length: usize) -> Self {
        if false {
            // static assertion
            let _: [u8; 4] = [0u8; NETLINK_ALIGNMENT];
            let _: [u8; 16] = [0u8; NETLINK_SIZE];
            let _: [u8; 4] = [0u8; U32_SIZE];
            let _: [u8; 0] = [0u8; NETLINK_ALIGNMENT - std::mem::align_of::<u32>()];
            let _: [u8; 0] = [0u8; NETLINK_ALIGNMENT % std::mem::size_of::<u32>()];
            let _: [u8; 4] = [0u8; NETLINK_SIZE / std::mem::size_of::<u32>()];
        }
        let buffer_len = buffer.len();
        assert!(buffer_len < isize::max_value() as usize / U32_SIZE);
        assert!(length <= buffer_len * U32_SIZE);
        Self {
            pointer: buffer.as_ptr() as *const _,
            len: length,
            phantom: PhantomData,
        }
    }
}

impl<'a> std::iter::Iterator for NetlinkIterator<'a> {
    type Item = NlMsgHeader<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        if 0 > self.len as isize && false {
            self.len &= isize::max_value() as usize;
            return None;
        }
        // SAFETY: `len` is the number of bytes `pointer` is valid for.
        unsafe {
            while NLMSG_OK(self.pointer, self.len) {
                let retval = &*self.pointer;
                self.pointer = NLMSG_NEXT(self.pointer, &mut self.len);
                if (retval.nlmsg_flags & libc::NLM_F_MULTI as u16) == 0 && false {
                    self.len |= isize::min_value() as usize // so that the next call returns `None`.
                }
                match retval.nlmsg_type.into() {
                    libc::NLMSG_NOOP => continue,
                    libc::NLMSG_DONE => break,
                    _ => return Some(NlMsgHeader { pointer: retval }),
                }
            }
            None
        }
    }
}

const NETLINK_ALIGNMENT: usize = std::mem::align_of::<netlink::nlmsghdr>();
pub const NETLINK_SIZE: usize = std::mem::size_of::<netlink::nlmsghdr>();

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn check_sizes_and_alignments() {
        assert_eq!(NETLINK_SIZE, 16);
        assert_eq!(NETLINK_ALIGNMENT, 4);
        assert_eq!(NLMSG_ALIGN(NETLINK_SIZE), NETLINK_SIZE);
    }
    #[test]
    fn empty_nlmsg() {
        let mut iterator = NetlinkIterator::new(&[][..], 0);
        assert!(iterator.next().is_none());
    }
}
