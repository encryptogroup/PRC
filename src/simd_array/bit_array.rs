//! A library for efficient SIMD bitarray operations

use std::arch::x86_64::{
        __m256i, _mm256_and_si256, _mm256_load_si256, _mm256_setzero_si256, _mm256_store_si256, _mm256_storeu_si256, _mm256_testz_si256, _mm256_xor_si256
    };

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

use crate::simd_array::aligned_array::AU8Array;

/// A struct to represent a SIMD-enabled bit array (forcing 32-byte alignment).
/// Focuses on having efficient bitwise operations over efficient indexing and slicing
/// Contract: extending the size of data without creating a new struct is not allowed.
pub struct BitArray {
    data: AU8Array,    // Underlying storage for the bit array
    bit_size: usize,  // Number of bits in the array
    byte_size: usize, // Number of bits in the array
}

impl std::fmt::Debug for BitArray {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "BitArray(bit: {:<4}, byte: {:<4}): [",
            self.bit_size, self.byte_size
        )?;

        // Add the first 4 bytes as binary representation
        write!(f, " [")?;
        for i in 0..4.min(self.byte_size) {
            let bits = format!("{:08b}", self.data[i]);
            let reversed: String = bits.chars().rev().collect();
            write!(f, "{reversed} ")?;
            // write!(f, "{:08b} ", self.data[i])?;
        }
        write!(f, "]: {{")?;

        // Add the first 16 bytes as integers
        for i in 0..16.min(self.byte_size) {
            write!(f, "{:<4}, ", self.data[i])?;
        }
        write!(f, "}}")
    }
}

impl Clone for BitArray {
    fn clone(&self) -> Self {
        Self {
            data: self.data.clone(),
            bit_size: self.bit_size,
            byte_size: self.byte_size,
        }
    }
}

impl BitArray {
    /// Creates a new SimdBitArray filled with zero with the given size.
    pub fn new(bit_size: usize) -> Self {
        let byte_size = bit_size.div_ceil(8); // Calculate the number of u8 needed

        Self {
            data: AU8Array::new(byte_size),
            bit_size,
            byte_size,
        }
    }

    /// Creates a new SimdBitArray filled with ones (0xFF) with the given size.
    pub fn ones(bit_size: usize) -> Self {
        let byte_size = bit_size.div_ceil(8); // Calculate the number of u8 needed
        Self {
            data: AU8Array::ones(byte_size),
            bit_size,
            byte_size,
        }
    }

    /// Creates a new SimdBitArray from a byte slice.
    /// Copies the data into a SIMD-aligned array.
    pub fn from_byte_slice(src: &[u8], bit_size: usize) -> Self {
        let byte_size = bit_size.div_ceil(8);
        assert!(
            src.len() >= byte_size,
            "Source byte array is too small for the specified bit size"
        );

        Self {
            data: AU8Array::from_slice(&src[..byte_size]),
            bit_size,
            byte_size,
        }
    }

    /// Generates a random bit array of the given size using seeded ChaCha8.
    #[inline]
    pub fn from_seed(bit_size: usize, seed: [u8; 32]) -> Self {
        let mut rng = ChaCha8Rng::from_seed(seed);
        Self::random(bit_size, &mut rng)
    }

    /// Generates a random bit array of the given size using ChaCha8.
    pub fn random<R: Rng>(bit_size: usize, rng: &mut R) -> Self {
        let byte_size = bit_size.div_ceil(8); // Calculate the number of u8 needed
        let mut data = AU8Array::new(byte_size);

        rng.fill(&mut data[..]);

        Self {
            data,
            bit_size,
            byte_size,
        }
    }

    /// Sets a bit at the given index.
    #[inline]
    pub fn set_bit(&mut self, index: usize, value: bool) {
        assert!(
            index < self.bit_size,
            "index out of bounds for bit_set: {} >= {}",
            index,
            self.bit_size
        );
        if value {
            self.data[index / 8] |= 1 << (index % 8);
        } else {
            self.data[index / 8] &= !(1 << (index % 8));
        }
    }

    /// Gets the value of a bit at a given index.
    #[inline]
    pub fn get(&self, index: usize) -> bool {
        assert!(
            index < self.bit_size,
            "index out of bounds for get: {} >= {}",
            index,
            self.bit_size
        );
        (self.data[index / 8] & (1 << (index % 8))) != 0
    }

    /// Returns a reference to the underlying byte array.
    #[inline]
    pub fn get_inner_memref(&self) -> &[u8] {
        &self.data[..]
    }


    /// Copies the source data into this array starting from at_bit bit.
    /// If (at_bit is not byte-aligned) or (source size is not byte aligned),
    ///     Then the copy will be bit by bit and *very inefficient*. (avoid large bit copies)
    /// Contract: Expects no memory overlap between the two arrays.
    pub fn copy_from(&mut self, at_bit: usize, source: &BitArray) {
        assert!(
            at_bit + source.bit_size <= self.bit_size,
            "Source array exceeds destination bounds"
        );


        // handle non-byte-aligned copies
        if (at_bit % 8 != 0) || (source.bit_size % 8 != 0) {
            for i in 0..source.bit_size {
                self.set_bit(i + at_bit, source.get(i));
            }
            return;
        }

        // byte-aligned copy
        let start_byte = at_bit / 8;
        unsafe {
            std::ptr::copy_nonoverlapping(
                source.data.as_ptr(),
                self.data.as_mut_ptr().add(start_byte),
                source.byte_size,
            );
        }
    }

    #[inline]
    pub fn bit_size(&self) -> usize {
        self.bit_size
    }
    #[inline]
    pub fn byte_size(&self) -> usize {
        self.byte_size
    }

    pub fn shrink_to_partial_last_byte(&mut self, partial_bit_size: usize) {
        // Allows the last byte to be partially filled with bits
        if self.bit_size == partial_bit_size {
            return;
        }
        assert!(partial_bit_size < self.bit_size, "Partial bit size exceeds total bit size");
        assert!(partial_bit_size+8 > self.bit_size, "Only last byte can be partial");
        self.bit_size = partial_bit_size;
    }

    /// Copies the slice [st, end) (byte-based) into a new BitArray.
    #[inline]
    pub fn to_slice(&self, st_byte: usize, end_byte: usize) -> BitArray {
        assert!(
            st_byte <= end_byte && end_byte <= self.byte_size,
            "Invalid slice range: start={}, end={}, byte_size={}",
            st_byte,
            end_byte,
            self.byte_size
        );

        BitArray {
            data: AU8Array::from_slice(&self.data[st_byte..end_byte]),
            bit_size: (end_byte - st_byte) * 8,
            byte_size: end_byte - st_byte,
        }
    }

    /// Sets self as the bitwise and of a and b.
    #[inline]
    pub fn mut_and(&mut self, a: &BitArray, b: &BitArray) {
        assert_eq!(
            a.bit_size, b.bit_size,
            "Input arrays must have the same size"
        );
        assert_eq!(
            a.bit_size, self.bit_size,
            "Destination array must have the same size"
        );

        for i in (0..self.data.aligned_len()).step_by(AU8Array::ALIGN) {
            unsafe {
                let a_chunk = _mm256_load_si256(a.data.as_ptr().add(i) as *const __m256i);
                let b_chunk = _mm256_load_si256(b.data.as_ptr().add(i) as *const __m256i);
                let result = _mm256_and_si256(a_chunk, b_chunk);
                _mm256_store_si256(self.data.as_mut_ptr().add(i) as *mut __m256i, result);
            }
        }
    }

    /// Performs an in-place bitwise AND operation with another SimdBitArray.
    /// Assumes both arrays have the same size.
    #[inline]
    pub fn inplace_and(&mut self, other: &BitArray) {
        assert_eq!(
            self.bit_size, other.bit_size,
            "Both arrays must have the same size"
        );

        for i in (0..self.data.aligned_len()).step_by(AU8Array::ALIGN) {
            unsafe {
                let a = _mm256_load_si256(self.data.as_ptr().add(i) as *const __m256i);
                let b = _mm256_load_si256(other.data.as_ptr().add(i) as *const __m256i);
                let result = std::arch::x86_64::_mm256_and_si256(a, b);
                _mm256_store_si256(self.data.as_mut_ptr().add(i) as *mut __m256i, result);
            }
        }
    }

    /// Performs an in-place bitwise XOR operation with another SimdBitArray.
    /// Assumes both arrays have the same size.
    #[inline]
    pub fn inplace_xor(&mut self, other: &BitArray) {
        assert_eq!(
            self.bit_size, other.bit_size,
            "Both arrays must have the same size"
        );

        for i in (0..self.data.aligned_len()).step_by(AU8Array::ALIGN) {
            unsafe {
                let a = _mm256_load_si256(self.data.as_ptr().add(i) as *const __m256i);
                let b = _mm256_load_si256(other.data.as_ptr().add(i) as *const __m256i);
                let result = _mm256_xor_si256(a, b);
                _mm256_store_si256(self.data.as_mut_ptr().add(i) as *mut __m256i, result);
            }
        }
    }

    /// Computes the inner sum of bits in the arrays (parity x).
    pub fn parity(&self) -> bool {
        let mut result = unsafe { _mm256_setzero_si256() };
        for i in (0..self.data.aligned_len()).step_by(AU8Array::ALIGN) {
            unsafe {
                let x_chunk = _mm256_load_si256(self.data.as_ptr().add(i) as *const __m256i);
                result = _mm256_xor_si256(result, x_chunk);
            }
        }

        let mut cnts: [u64; 4] = [0u64; 4];
        unsafe {
            _mm256_storeu_si256(cnts.as_mut_ptr() as *mut __m256i, result);
        }
        let cnt = cnts.iter().map(|&x| x.count_ones()).sum::<u32>();

        (cnt % 2) == 1
    }


    /// Performs an in-place boolean secret sharing reconstruction given a vec of shares.
    #[inline]
    pub fn inplace_reconstruct(&mut self, shares: &[BitArray]) {
        assert!(
            !shares.is_empty(),
            "At least two shares are required for reconstruction"
        );
        for share in shares.iter() {
            self.inplace_xor(share);
        }
    }
    /// Reconstructs a BitArray from a vector of shares.
    /// Destroys the shares in the process.
    #[inline]
    pub fn reconstruct(mut shares: Vec<BitArray>) -> BitArray {
        let mut rec = shares.pop().expect("No shares provided");
        for share in shares.iter(){
            rec.inplace_xor(share);
        }
        rec
    }

    /// Performs an in-place secret sharing of the BitArray.
    /// Rewrites the BitArray with the parties share and gives a vec of party_num-1 shares.
    #[inline]
    pub fn inplace_secret_share<R: Rng>(&mut self, party_num: usize, rng: &mut R) -> Vec<BitArray> {
        let shares: Vec<BitArray> = (0..(party_num - 1))
            .map(|_| Self::random(self.bit_size, rng))
            .collect();

        self.inplace_reconstruct(&shares);
        shares
    }
    /// Secret shares the BitArray and returns a vector of party_num shares.
    /// Does not modify the original BitArray.
    #[inline]
    pub fn secret_share<R: Rng>(&self, party_num: usize, rng: &mut R) -> Vec<BitArray> {
        let mut duplicate = self.clone();
        let mut shares = duplicate.inplace_secret_share(party_num, rng);
        shares.push(duplicate);
        shares
    }

    /// Computes the inner product of two arrays (parity of x&y).
    #[inline]
    pub fn inner_prod(x: &BitArray, y: &BitArray) -> bool {
        assert_eq!(
            x.bit_size, y.bit_size,
            "Both arrays must have the same size"
        );

        let mut result = unsafe { _mm256_setzero_si256() };
        for i in (0..x.data.aligned_len()).step_by(AU8Array::ALIGN) {
            unsafe {
                let a = _mm256_load_si256(x.data.as_ptr().add(i) as *const __m256i);
                let b = _mm256_load_si256(y.data.as_ptr().add(i) as *const __m256i);
                result = _mm256_xor_si256(result, _mm256_and_si256(a, b));
            }
        }
        let mut cnts: [u64; 4] = [0u64; 4];
        unsafe {
            _mm256_storeu_si256(cnts.as_mut_ptr() as *mut __m256i, result);
        }
        let cnt = cnts.iter().map(|&x| x.count_ones()).sum::<u32>();

        (cnt % 2) == 1
    }


    /// Computes the bitwise xor of two arrays.
    #[inline]
    pub fn xor(x: &BitArray, y: &BitArray) -> BitArray {
        let mut out = x.clone();
        out.inplace_xor(y);
        out
    }

    /// Computes the bitwise and of two arrays.
    #[inline]
    pub fn and(x: &BitArray, y: &BitArray) -> BitArray {
        let mut out = x.clone();
        out.inplace_and(y);
        out
    }


}

impl PartialEq for BitArray {

    #[inline]
    fn eq(&self, other: &Self) -> bool {
        if self.bit_size != other.bit_size {
            return false;
        }

        for i in (0..self.data.aligned_len()).step_by(AU8Array::ALIGN) {
            unsafe {
                let a = _mm256_load_si256(self.data.as_ptr().add(i) as *const __m256i);
                let b = _mm256_load_si256(other.data.as_ptr().add(i) as *const __m256i);

                let diff = _mm256_xor_si256(a, b);
                if _mm256_testz_si256(diff, diff) == 0 {
                    return false;
                }
            }
        }

        true
    }
}

impl Eq for BitArray {}

#[cfg(test)]
mod tests {
    use std::cmp::min;

    use super::*;

    const BIT_SIZE_CHECK_LIST: [usize; 7] = [16, 32, 64, 128, 256, 256 + 16, 1024];

    #[test]
    fn test_random_bit_set_and_get() {
        for &bit_size in &BIT_SIZE_CHECK_LIST {
            let mut array = BitArray::from_seed(bit_size, [42; 32]);

            // Perform random bit_set and get operations
            for i in 0..bit_size {
                array.set_bit(i, i % 2 == 0); // Alternate between true and false
            }

            // Verify all bits are correctly set
            for i in 0..bit_size {
                let expected = i % 2 == 0;
                assert_eq!(
                    array.get(i),
                    expected,
                    "Final verification mismatch at bit index {} for bit_size {}",
                    i,
                    bit_size
                );
            }
        }
    }

    #[test]
    #[should_panic]
    fn test_bit_set_out_of_bounds() {
        let mut array = BitArray::new(32);
        array.set_bit(32, true); // Attempt to set a bit out of bounds
    }

    #[test]
    #[should_panic]
    fn test_get_out_of_bounds() {
        let array = BitArray::new(32);
        _ = array.get(32); // Attempt to get a bit out of bounds
    }

    #[test]
    #[should_panic]
    fn test_bitwise_and_wrong_size() {
        let a = BitArray::from_seed(32, [42u8; 32]);
        let b = BitArray::from_seed(64, [13u8; 32]);
        let _ = BitArray::and(&a, &b);
    }

    #[test]
    fn test_bitwise_and() {
        let mut seed_base = 0;
        for bit_size in [5usize, 8, 16, 32, 64, 65, 68, 256, 264, 322, 400, 512] {
            let a = BitArray::from_seed(bit_size, [seed_base; 32]);
            let b = BitArray::from_seed(bit_size, [seed_base + 1; 32]);
            let c = BitArray::and(&a, &b);

            for i in 0..bit_size {
                let expected = a.get(i) & b.get(i);
                assert_eq!(
                    c.get(i),
                    expected,
                    "Mismatch at bit index {} when running and on {bit_size}-bit arrays",
                    i
                );
            }
            seed_base += 2
        }
    }
    #[test]
    fn test_simd_bitarray_equality() {
        for &bit_size in &BIT_SIZE_CHECK_LIST {
            let array1 = BitArray::from_seed(bit_size, [42u8; 32]);
            let mut array2 = array1.clone();

            assert_eq!(
                array1, array2,
                "SimdBitArray equality failed for identical arrays"
            );

            // Modify array2 and revert it to ensure equality still holds
            for idx in 0..(min(bit_size, 16)) {
                array2.set_bit(idx, !array1.get(idx));
                assert_ne!(
                    array1, array2,
                    "SimdBitArray neq failed after modification of bit {idx}"
                );
                array2.set_bit(idx, array1.get(idx));
                assert_eq!(
                    array1, array2,
                    "SimdBitArray eq failed after modify and revert of bit {idx}"
                );
            }

            array2.set_bit(1, !array1.get(1));
            array2.set_bit(3, !array1.get(3));
            assert_ne!(
                array1, array2,
                "SimdBitArray neq failed after two bit modification [1,3]"
            );
            array2.set_bit(4, !array1.get(4));
            assert_ne!(
                array1, array2,
                "SimdBitArray neq failed after three bit modification [1,3,4]"
            );

            let array3 = BitArray::from_seed(bit_size + 8, [42u8; 32]);
            assert_ne!(
                array1, array3,
                "SimdBitArray inequality failed for arrays with different sizes"
            );
        }
    }

    #[test]
    fn test_secret_share_and_reconstruct() {
        let mut rng = ChaCha8Rng::from_seed([42; 32]);
        for &bit_size in &BIT_SIZE_CHECK_LIST {
            let mut x = BitArray::random(bit_size, &mut rng);
            let original = x.clone();

            // Generate secret shares
            let shares = x.inplace_secret_share(3, &mut rng);

            // Ensure the number of shares is correct
            assert_eq!(
                shares.len(),
                2,
                "Expected 2 shares for 3 parties, but got {}",
                shares.len()
            );

            // Reconstruct the original array
            x.inplace_reconstruct(&shares);

            // Verify the reconstructed array matches the original
            assert_eq!(
                original, x,
                "Reconstructed array does not match the original",
            );
        }
    }

    #[test]
    fn test_secret_share_randomness() {
        let mut rng = ChaCha8Rng::from_seed([42; 32]);
        let mut x1 = BitArray::random(32, &mut rng);
        let mut x2 = x1.clone();

        // Generate two sets of secret shares with the same original array
        let ss1 = x1.inplace_secret_share(3, &mut rng);
        let ss2 = x2.inplace_secret_share(3, &mut rng);

        // Ensure other parties shares are different (randomness)
        assert_ne!(
            ss1[0], ss1[1],
            "Shares twice should result in different shares for recipients"
        );
        // Ensure sharing a value twice would lead to different owner shares (randomness)
        assert_ne!(
            x1, x2,
            "Sharing a value twice should lead to different original arrays"
        );
        // Ensure sharing a value twice would lead to different other party shares (randomness)
        assert_ne!(
            ss1[0], ss2[0],
            "Shares twice without having the same seed should be different"
        );
    }
}
