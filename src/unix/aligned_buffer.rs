#![allow(unsafe_code)]
#[derive(Copy, Clone, Debug)]
pub struct U32AlignedBuffer<'a> {
    ptr: *const u8,
    len: i32,
    phantom: core::marker::PhantomData<&'a [u8]>,
}

#[cfg(target_os = "linux")]
const ALIGN: usize = 4;
#[cfg(not(target_os = "linux"))]
const ALIGN: usize = 8;

/// Indicate that *any* suitably aligned byte sequence is valid for this type.
///
/// # Safety
///
/// This trait is unsafe to implement. If it is implemented for a type that has
/// invalid values, safe methods of `U32AlignedBuffer` can invoke undefined
/// behavior.
pub unsafe trait FromBuffer: Copy + Sized {
    fn len(&self, size: usize) -> u32;
}

impl<'a> U32AlignedBuffer<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        let (ptr, len) = (buf.as_ptr(), buf.len());
        assert!(ptr as usize % ALIGN == 0, "misaligned buffer");
        // If the amount of data is too large, `read` could experience memory
        // unsafety due to integer overflow. This prevents this.
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
        assert!(size_of!(T) < 1024, "no type in the crate is this big");
        assert!(align_of!(T) <= ALIGN, "type has too strict alignment needs");
        // This is a debug_assert, not a full assert, because it cannot actually
        // fire. Each place where `self.ptr` is set checks its alignment.
        debug_assert!(self.ptr as usize % ALIGN == 0);
        // Check that the length is sufficiently large. A negative length is
        // obviously not sufficiently large.
        if self.len < size {
            return None;
        }
        // It is now safe to read the value of `self.ptr`. The contract of
        // `FromBuffer` guarantees that any aligned sequence of bytes of length
        // sizeof!(T) is a valid T. Since `FromBuffer` is a subtrait of `Copy`,
        // it is safe to do the bitwise copy here.
        //
        // We could use `read` instead of `read_unaligned`, but the latter is
        // safer in case of a bug.
        let output: T = unsafe { std::ptr::read_unaligned(self.ptr as _) };
        let len = output.len(self.len as _) as i32;
        // Check that `output` is valid.
        if len < size || len > self.len {
            return None;
        }
        let buffer = Self {
            len: len - size,
            // ptr is aligned, as is size, so ptr.offset(size) will be as well.
            // furthermore, size was already checked to be within bounds.
            ptr: unsafe { self.ptr.offset(size as _) },
            phantom: core::marker::PhantomData,
        };
        // Align `len` to the next suitably aligned value.
        // The length check in `new` ensures that this cannot overflow.
        let advance = ((len as usize + (ALIGN - 1)) & !(ALIGN - 1)) as i32;
        debug_assert!(advance % ALIGN as i32 == 0);
        // len going negative is safe
        self.len -= advance;
        // advance is guaranteed to be aligned, since we mask it with !(ALIGN - 1).
        // and ALIGN is a power of 2. Therefore, if `self.ptr` is aligned
        // initially, it will be after this call to `offset` is made. Since
        // `self.ptr` will be aligned when `Self` is constructed, it will
        // continue to be aligned by mathematical induction.
        //
        // Note that `self.ptr` may go up to ALIGN - 1 bytes out of bounds, but
        // it will never be dereferenced in this case. This is why
        // `wrapping_offset` is used instead of `offset`.
        self.ptr = self.ptr.wrapping_offset(advance as _);
        debug_assert!(self.ptr as usize % ALIGN == 0);
        Some((output, buffer))
    }
}

impl<'a> core::convert::TryFrom<U32AlignedBuffer<'a>> for std::net::IpAddr {
    type Error = ();

    fn try_from(s: U32AlignedBuffer<'a>) -> Result<Self, Self::Error> {
        match s.len {
            16 => unsafe {
                // SAFETY: we just did the bounds check.
                let v: [u8; 16] = core::ptr::read(s.ptr as _);
                Ok(v.into())
            },
            4 => unsafe {
                // SAFETY: we just did the bounds check.
                let v: [u8; 4] = core::ptr::read(s.ptr as _);
                Ok(v.into())
            },
            _ => Err(()),
        }
    }
}
