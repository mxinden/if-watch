use crate::aligned_buffer::{U32AlignedBuffer, FromBuffer};

#[derive(Debug)]
pub struct NetlinkIterator<'a>(U32AlignedBuffer<'a>);

unsafe impl FromBuffer for libc::nlmsghdr {
  fn len(&self, _: usize) -> u32 {
      self.nlmsg_len
  }
}

impl<'a> NetlinkIterator<'a> {
    pub fn new(buffer: &'a [u8]) -> NetlinkIterator<'a> {
        Self(U32AlignedBuffer::new(buffer))
    }
}

impl<'a> std::iter::Iterator for NetlinkIterator<'a> {
    type Item = (libc::nlmsghdr, U32AlignedBuffer<'a>);
    fn next(&mut self) -> Option<Self::Item> {
        self.0.read()
    }
}
