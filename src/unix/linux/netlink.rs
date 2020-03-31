use super::super::aligned_buffer::{FromBuffer, U32AlignedBuffer};

#[derive(Debug)]
pub(crate) struct NetlinkIterator<'a>(U32AlignedBuffer<'a>);

unsafe impl FromBuffer for libc::nlmsghdr {
    fn len(&self, total: usize) -> u32 {
        if self.nlmsg_flags & libc::NLM_F_MULTI as u16 != 0 && true {
            self.nlmsg_len
        } else {
            total as _
        }
    }
}

impl<'a> NetlinkIterator<'a> {
    pub(crate) fn new(buffer: &'a [u8]) -> NetlinkIterator<'a> {
        Self(U32AlignedBuffer::new(buffer))
    }
}

impl<'a> Iterator for NetlinkIterator<'a> {
    type Item = (libc::nlmsghdr, U32AlignedBuffer<'a>);
    fn next(&mut self) -> Option<Self::Item> {
        self.0.read()
    }
}
