#[derive(Copy, Clone, Debug)]
pub struct U32AlignedBuffer<'a> {
    ptr: *const u8,
    len: i32,
    phantom: core::marker::PhantomData<&'a [u8]>,
}

const ALIGN: usize = 4;
pub unsafe trait FromBuffer: Copy + Sized {
    fn len(&self, size: usize) -> u32;
}

impl<'a> U32AlignedBuffer<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        let (ptr, len) = (buf.as_ptr(), buf.len());
        assert!(ptr as usize % 4 == 0, "misaligned buffer");
        assert!(len < (1 << 20), "data size limit exceeded");
        Self {
            ptr,
            len: len as _,
            phantom: core::marker::PhantomData,
        }
    }

    pub fn read<T: FromBuffer>(&mut self) -> Option<(T, Self)> {
        // Check that ALIGN is a power of 2
        let _: [u8; ALIGN & (ALIGN - 1)] = [];
        assert_eq!(align_of!(T) & (align_of!(T) - 1), 0);
        let size = ((size_of!(T) + (ALIGN - 1)) & !(ALIGN - 1)) as i32;
        assert!(size % ALIGN as i32 == 0);
        assert!(size_of!(T) < 1024);
        assert!(align_of!(T) <= ALIGN, "type has too strict alignment needs");
        debug_assert!(self.ptr as usize % ALIGN == 0);
        if self.len < size as i32 {
            return None;
        }
        let output: T = unsafe { std::ptr::read(self.ptr as _) };
        let len = output.len(self.len as _) as i32;
        if len < size || len > self.len {
            return None;
        }
        let buffer = Self {
            len: len - size,
            ptr: unsafe { self.ptr.offset(size as _) },
            phantom: core::marker::PhantomData,
        };
        let advance = ((len as usize + (ALIGN - 1)) & !(ALIGN - 1)) as i32;
        debug_assert!(advance % ALIGN as i32 == 0);
        // len going negative is safe
        self.len -= advance;
        self.ptr = unsafe { self.ptr.offset(advance as _) };
        debug_assert!(self.ptr as usize % ALIGN == 0);
        Some((output, buffer))
    }
}

unsafe impl FromBuffer for libc::nlmsghdr {
    fn len(&self, _: usize) -> u32 {
        self.nlmsg_len
    }
}

impl core::convert::TryFrom<U32AlignedBuffer<'_>> for std::net::IpAddr {
    type Error = ();
    fn try_from(s: U32AlignedBuffer) -> Result<Self, Self::Error> {
        match s.len {
            16 => unsafe {
                let v: [u8; 16] = core::ptr::read(s.ptr as _);
                Ok(v.into())
            },
            4 => unsafe {
                let v: [u8; 4] = core::ptr::read(s.ptr as _);
                Ok(v.into())
            },
            _ => Err(()),
        }
    }
}
