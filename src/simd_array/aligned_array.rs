use std::alloc::{Layout, alloc, alloc_zeroed, dealloc};
use std::arch::x86_64::{__m256i, _mm256_load_si256, _mm256_loadu_si256, _mm256_store_si256};
use std::convert::{AsMut, AsRef};
use std::ops::{Deref, DerefMut};
use std::ptr::NonNull;
use std::slice;


/// A wrapper for [u8] that is guaranteed to be aligned to 32 bytes.
/// This struct ensures that the underlying memory is compatible with AVX2 simd operations
/// enabling aligned load/store and padding length to a multiple 32 bytes. 
pub struct AU8Array {
    ptr: NonNull<u8>,
    len: usize,
}

impl AU8Array {
    /// The alignment in bytes for the underlying memory.
    pub const ALIGN: usize = 32;

    /// AU8box handles memory layout manually. 
    /// Ensure alignment and max length that is a multiple of the alignment (without changing 
    /// the original length).
    #[inline]
    fn create_layout(mut len: usize) -> Layout {
        assert!(len > 0, "Length must be > 0");
        if len % Self::ALIGN != 0 {
            len += Self::ALIGN - (len % Self::ALIGN);
        }
        Layout::from_size_align(len, Self::ALIGN).expect("Invalid layout")
    }

    /// Create an empty AU8Box with no allocation. 
    /// Useful for having zero-length iterators or similar use cases.
    fn empty() -> Self {
        Self {
            ptr: NonNull::dangling(),
            len: 0,
        }
    }

    /// Create a new AU8Box filled with zero with the specified length.
    pub fn new(len: usize) -> Self {
        let layout = Self::create_layout(len);

        unsafe {
            let raw_ptr = alloc_zeroed(layout);
            if raw_ptr.is_null() {
                std::alloc::handle_alloc_error(layout);
            }

            Self {
                ptr: NonNull::new_unchecked(raw_ptr),
                len,
            }
        }
    }

    pub fn clone(&self) -> Self {
        let layout = Self::create_layout(self.len);
        unsafe {
            let raw_ptr = alloc(layout);
            if raw_ptr.is_null() {
                std::alloc::handle_alloc_error(layout);
            }

            // Copy the data from the original pointer to the new pointer
            // using SIMD operations for performance, having the alignment and max size compatible
            // with AVX2 instructions (32-byte alignment) is guaranteed by the layout. When len is
            // not a multiple of 32, we have already allocated the remaining bytes as max size.
            for i in (0..self.len).step_by(Self::ALIGN) {
                let chunk = _mm256_load_si256(self.ptr.as_ptr().add(i) as *const __m256i);
                _mm256_store_si256(raw_ptr.add(i) as *mut __m256i, chunk);
            }

            Self {
                ptr: NonNull::new_unchecked(raw_ptr),
                len: self.len,
            }
        }
    }

    /// Create an AU8Box from a slice, copying all data into an aligned memory.
    pub fn from_slice(slice: &[u8]) -> Self {
        let len = slice.len();
        if len == 0 {
            return Self::empty();
        }

        let layout = Self::create_layout(len);
        unsafe {
            let raw_ptr = alloc(layout);
            if raw_ptr.is_null() {
                std::alloc::handle_alloc_error(layout);
            }

            // No guarantee that the slice is aligned, so we use unaligned load and handle max_size not a multiple of 32
            let simd_pass = len - (len % Self::ALIGN);
            for i in (0..simd_pass).step_by(Self::ALIGN) {
                let chunk = _mm256_loadu_si256(slice.as_ptr().add(i) as *const __m256i);
                _mm256_store_si256(raw_ptr.add(i) as *mut __m256i, chunk); // alignment guaranteed
            }

            // Handle remaining bytes if byte_size is not a multiple of 32
            if len % Self::ALIGN != 0 {
                // Initialize the remaining bytes to zero till byte alignment which is a multiple of 32 since
                // we use alloc instead of alloc_zeroed
                std::ptr::write_bytes(raw_ptr.add(simd_pass), 0, Self::ALIGN);

                let current_byte = simd_pass;
                std::ptr::copy_nonoverlapping(
                    slice.as_ptr().add(current_byte),
                    raw_ptr.add(current_byte),
                    len - current_byte,
                );
            }

            Self {
                ptr: NonNull::new_unchecked(raw_ptr),
                len,
            }
        }
    }

    /// Create an AU8Box filled with ones (0xFF) for the given length.
    pub fn ones(len: usize) -> Self {
        let layout = Self::create_layout(len);

        unsafe {
            let raw_ptr = alloc(layout);
            if raw_ptr.is_null() {
                std::alloc::handle_alloc_error(layout);
            }
            let aligned_len = Self::align_max_len(len);
            std::ptr::write_bytes(
                raw_ptr,
                0xFF, // Fill with ones (0xFF for each byte)
                aligned_len,
            );
            Self {
                ptr: NonNull::new_unchecked(raw_ptr),
                len,
            }
        }
    }

    /// Determine the size with padding to ensure a length that is a multiple of the alignment.
    #[inline]
    fn align_max_len(l: usize) -> usize {
        if l % Self::ALIGN == 0 {
            l
        } else {
            l + (Self::ALIGN - (l % Self::ALIGN))
        }
    }
    /// Return the length of the AU8Box, which includes alignment padding.
    #[inline]
    pub fn aligned_len(&self) -> usize {
        Self::align_max_len(self.len)
    }
}

// Support to behave like [u8]
impl Deref for AU8Array {
    type Target = [u8];

    #[inline]
    fn deref(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }
}
impl DerefMut for AU8Array {
    #[inline]
    fn deref_mut(&mut self) -> &mut [u8] {
        unsafe { slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len) }
    }
}

impl AsRef<[u8]> for AU8Array {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        self
    }
}
impl AsMut<[u8]> for AU8Array {
    #[inline]
    fn as_mut(&mut self) -> &mut [u8] {
        self
    }
}

unsafe impl Send for AU8Array {}
unsafe impl Sync for AU8Array {}

// Auto deallocate by recomputing Layout
impl Drop for AU8Array {
    fn drop(&mut self) {
        if self.len == 0 {
            return; // No allocation to deallocate
        }
        unsafe {
            let layout = Layout::from_size_align(self.len, Self::ALIGN).unwrap();
            dealloc(self.ptr.as_ptr(), layout);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_box_len_and_alignment() {
        let len = 100;
        let b = AU8Array::new(len);
        assert_eq!(b.len, len);
        assert_eq!((b.ptr.as_ptr() as usize) % AU8Array::ALIGN, 0);
    }

    #[test]
    fn test_index_and_index_mut() {
        let mut b = AU8Array::new(8);
        for i in 0..8 {
            b[i] = i as u8;
        }
        for i in 0..8 {
            assert_eq!(b[i], i as u8);
        }
    }

    #[test]
    fn test_deref_and_deref_mut() {
        let mut b = AU8Array::new(4);
        b[0] = 10;
        b[1] = 20;
        b[2] = 30;
        b[3] = 40;
        let slice: &[u8] = &b;
        assert_eq!(slice, &[10, 20, 30, 40]);
        let slice_mut: &mut [u8] = &mut b;
        slice_mut[2] = 99;
        assert_eq!(b[2], 99);
    }

    #[test]
    fn test_as_ref_and_as_mut() {
        let mut b = AU8Array::new(3);
        b[0] = 1;
        b[1] = 2;
        b[2] = 3;
        let r: &[u8] = b.as_ref();
        assert_eq!(r, &[1, 2, 3]);
        let m: &mut [u8] = b.as_mut();
        m[1] = 42;
        assert_eq!(b[1], 42);
    }

    #[test]
    #[should_panic(expected = "Length must be > 0")]
    fn test_zero_length_panics() {
        let _ = AU8Array::new(0);
    }

    #[test]
    fn test_alignment_for_various_lengths() {
        for len in [1, 16, 31, 32, 33, 64, 100, 128] {
            let b = AU8Array::new(len);
            assert_eq!((b.ptr.as_ptr() as usize) % AU8Array::ALIGN, 0);
        }
    }
}
