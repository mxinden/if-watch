#![allow(non_snake_case)]

#[derive(Debug)]
pub struct NetlinkIterator<'a> {
    buffer: &'a [u32],
}

const U32_SIZE: usize = std::mem::size_of::<u32>();
pub const NLMSG_HDRLEN: usize = NLMSG_ALIGN(std::mem::size_of::<libc::nlmsghdr>());

#[inline]
const fn NLMSG_ALIGN(len: usize) -> usize {
    (len + NLMSG_ALIGNTO - 1) & !(NLMSG_ALIGNTO - 1)
}

#[inline]
pub(crate) const fn NLMSG_LENGTH(size: usize) -> usize {
    size + NLMSG_HDRLEN
}

pub(crate) const fn NLMSG_SPACE(size: usize) -> usize {
    NLMSG_ALIGN(NLMSG_LENGTH(size))
}

pub struct NlMsgHeader<'a> {
    header: &'a libc::nlmsghdr,
    data: &'a [u32],
}

impl<'a> NlMsgHeader<'a> {
    pub fn data(&self) -> &'a [u32] {
        self.data
    }
    pub fn header(&self) -> &libc::nlmsghdr {
        self.header
    }
}

impl<'a> NetlinkIterator<'a> {
    pub fn new(buffer: &'a [u32], length: usize) -> Self {
        let buffer_len = buffer.len();
        assert!(
            buffer_len < isize::max_value() as usize / U32_SIZE,
            "Netlink capacity overflow"
        );
        assert!(length <= buffer_len * U32_SIZE);
        Self { buffer }
    }
}

impl<'a> std::iter::Iterator for NetlinkIterator<'a> {
    type Item = NlMsgHeader<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        const NLMSG_ALIGNTO: u32 = 4;
        const MIN_BUFSIZE: usize = NLMSG_ALIGN(NETLINK_SIZE) / std::mem::size_of::<u32>();
        if self.buffer.len() < MIN_BUFSIZE {
            return None;
        }
        let _: [u8; NLMSG_ALIGNTO as _] = [0u8; std::mem::size_of::<u32>()];
        let _: [u8; NLMSG_ALIGNTO as _] = [0u8; std::mem::align_of::<libc::nlmsghdr>()];
        let _: [u8; NLMSG_ALIGNTO as _] = [0u8; std::mem::align_of::<u32>()];
        // SAFETY: *any* aligned sequence of 16 bytes is a valid nlmsghdr, and
        // our buffer is valid as long as this struct is.
        let msg: &'a libc::nlmsghdr = unsafe { &*(self.buffer.as_ptr() as *const _) };
        // use a signed comparison to prevent overflow later
        // this enforces an implicit 2GiB limit on message sizes
        if (msg.nlmsg_len as i32) < std::mem::size_of::<libc::nlmsghdr>() as i32 {
            return None;
        }
        let msg_len = ((msg.nlmsg_len + NLMSG_ALIGNTO - 1) / NLMSG_ALIGNTO) as usize;
        if msg_len > self.buffer.len() {
            return None;
        }
        let (data, new_buffer) = &self.buffer[4..].split_at(msg_len - 4);
        self.buffer = new_buffer;
        Some(NlMsgHeader { header: msg, data })
    }
}

const NLMSG_ALIGNTO: usize = 4;
pub const NETLINK_SIZE: usize = std::mem::size_of::<libc::nlmsghdr>();

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn check_sizes_and_alignments() {
        assert_eq!(NETLINK_SIZE, 16);
        assert_eq!(NLMSG_ALIGNTO, 4);
        assert_eq!(NLMSG_ALIGN(NETLINK_SIZE), NETLINK_SIZE);
    }
    #[test]
    fn empty_nlmsg() {
        let mut iterator = NetlinkIterator::new(&[][..], 0);
        assert!(iterator.next().is_none());
    }
}
