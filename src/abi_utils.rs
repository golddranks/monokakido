use core::{
    mem::{align_of, size_of},
    slice,
};

use crate::Error;

#[repr(transparent)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct LE32(u32);

impl LE32 {
    pub fn read(self) -> u32 {
        u32::from_le(self.0)
    }

    pub fn us(self) -> usize {
        self.read() as usize
    }

    pub fn from(slice: &[u8]) -> Result<(Self, &[u8]), Error> {
        if slice.len() < size_of::<LE32>() {
            return Err(Error::BufferTooSmall);
        }
        let (le32, tail) = slice.split_at(size_of::<LE32>());
        Ok((LE32(u32::from_ne_bytes(le32.try_into().unwrap())), tail))
    }
}

impl From<u32> for LE32 {
    fn from(value: u32) -> Self {
        Self(u32::from_le(value))
    }
}

unsafe impl TransmuteSafe for LE32 {}

pub(crate) unsafe trait TransmuteSafe: Default + Clone {
    fn from_buf(buf: &[u8]) -> Result<(&Self, &[u8]), Error> {
        if buf.len() < size_of::<Self>() {
            return Err(Error::Transmute);
        }
        if buf.as_ptr() as usize % align_of::<Self>() != 0 {
            return Err(Error::Transmute);
        }
        let (me, tail) = buf.split_at(size_of::<Self>());
        let me = unsafe { &*(me.as_ptr() as *const Self) };
        Ok((me, tail))
    }

    fn slice_from_buf(buf: &[u8], n: usize) -> Result<(&[Self], &[u8]), Error> {
        if buf.len() < n * size_of::<Self>() {
            return Err(Error::Transmute);
        }
        if buf.as_ptr() as usize % align_of::<Self>() != 0 {
            return Err(Error::Transmute);
        }
        let tail = &buf[n * size_of::<Self>()..];
        let us: &[Self] = unsafe { slice::from_raw_parts(buf.as_ptr() as *const Self, n) };
        Ok((us, tail))
    }

    fn slice_as_bytes_mut(slice: &mut [Self]) -> &mut [u8] {
        unsafe {
            slice::from_raw_parts_mut(
                slice.as_mut_ptr() as *mut u8,
                slice.len() * size_of::<Self>(),
            )
        }
    }

    fn slice_as_bytes(slice: &[Self]) -> &[u8] {
        unsafe {
            slice::from_raw_parts(slice.as_ptr() as *const u8, slice.len() * size_of::<Self>())
        }
    }

    fn as_bytes_mut(&mut self) -> &mut [u8] {
        Self::slice_as_bytes_mut(slice::from_mut(self))
    }

    fn as_bytes(&self) -> &[u8] {
        Self::slice_as_bytes(slice::from_ref(self))
    }
}
