//! Support for 2 dimensional bit arrays, specifically for storing shares of beaver triplets.
//! This module provides the `D2BitArray` struct, which contains multiple `BitArray` instances.
//! This is useful in secure multi-party computation (MPC) scenarios where triplets of values are shared among parties.
//! We do not use the name Matrix as we do not support operations like matrix multiplication or addition.
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

use super::bit_array::BitArray;

/// Stores a vector of AVX2 SIMD-aligned Bitarrays
/// Focuses on high-performance bitwise operations, particularly for secure multi-party computation (MPC) scenarios.
/// Provide support for handling Beaver triplets
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct D2BitArray {
    arrays: Vec<BitArray>,
}

impl D2BitArray {
    /// Creates an empty D2BitArray.
    pub fn empty() -> D2BitArray {
        D2BitArray { arrays: Vec::new() }
    }
    /// Creates a new D2BitArray from a vector of BitArray.
    pub fn new(v: Vec<BitArray>) -> D2BitArray {
        D2BitArray { arrays: v }
    }
    /// Creates a D2BitArray with n BitArray instances, each initialized to zero.
    pub fn zeros(n: usize, bit_size: usize) -> D2BitArray {
        let mut arrays = Vec::with_capacity(n);
        for _ in 0..n {
            arrays.push(BitArray::new(bit_size));
        }
        D2BitArray { arrays }
    }

    /// Generate from a random seed
    /// the first 4 bytes of the seed are overwritten to have different seeds for each BitArray. 
    #[inline]
    pub fn from_seed(array_size: usize, bit_num: usize, seed: &[u8; 32]) -> D2BitArray {
        let mut rng = ChaCha8Rng::from_seed(*seed);
        Self::random(array_size, bit_num, &mut rng)
    }

    /// Generate from a random seed
    /// the first 4 bytes of the seed are overwritten to have different seeds for each BitArray. 
    pub fn random<R: Rng>(array_size: usize, bit_num: usize, rng: &mut R) -> D2BitArray {
        D2BitArray::new((0..array_size)
            .map(|_| BitArray::random(bit_num, rng))
            .collect())
    }

    /// Pushes a BitArray into the D2BitArray.
    #[inline]
    pub fn push(&mut self, array: BitArray) {
        self.arrays.push(array);
    }

    #[inline]
    pub fn array_num(&self) -> usize {
        self.arrays.len()
    }

    /// Returns the byte_size of the inner BitArray.
    /// Expects all inner arrays to have the same byte size.
    #[inline]
    pub fn inner_byte_size(&self) -> usize {
        if self.arrays.is_empty() {
            return 0;
        }
        self.arrays[0].byte_size()
    }

    /// Copies slices [st, end) (byte-based) from inner BitArrays and forms a new D2BitArray
    #[inline]
    pub fn to_slice(&self, st_byte: usize, end_byte: usize) -> D2BitArray {
        let mut sliced_arrays = Vec::with_capacity(self.arrays.len());
        for array in &self.arrays {
            sliced_arrays.push(array.to_slice(st_byte, end_byte));
        }
        D2BitArray::new(sliced_arrays)
    }

    /// Returns a reference to the BitArray at the given index.
    #[inline]
    pub fn get_array(&self, index: usize) -> &BitArray {
        assert!(
            index < self.arrays.len(),
            "Index out of bounds: the len is {} but the index is {}",
            self.arrays.len(),
            index
        );
        &self.arrays[index]
    }
    /// Returns a mutable reference to the BitArray at the given index.
    #[inline]
    pub fn get_mut_array(&mut self, index: usize) -> &mut BitArray {
        assert!(
            index < self.arrays.len(),
            "Index out of bounds: the len is {} but the index is {}",
            self.arrays.len(),
            index
        );
        &mut self.arrays[index]
    }
    
    #[inline]
    pub fn inplace_xor(&mut self, other: &D2BitArray) {
        assert!(
            self.arrays.len() == other.arrays.len(),
            "Both D2BitArrays must have the same number of BitArray"
        );

        for (self_array, other_array) in self.arrays.iter_mut().zip(other.arrays.iter()) {
            self_array.inplace_xor(other_array);
        }
    }

    #[inline]
    pub fn inplace_reconstruct(&mut self, shares: Vec<D2BitArray>)  {
        assert!(
            !shares.is_empty(),
            "At least two shares are required for reconstruction"
        );
        for share in shares.iter() {
            self.inplace_xor(share);
        }
    }
    #[inline]
    pub fn reconstruct(shares: Vec<D2BitArray>) -> D2BitArray {
        assert!(
            shares.len() > 1,
            "At least two shares are required for reconstruction"
        );

        let mut out = shares[0].clone();
        out.inplace_reconstruct(shares[1..].to_vec());
        out
    }


    /// Unpacks the D2BitArray into three mutable BitArrays, used for beaver triplets.
    #[inline]
    pub fn unpack_as_mut_beaver(&mut self) -> (&mut BitArray, &mut BitArray, &mut BitArray) {
        assert!(
            self.arrays.len() == 3,
            "D2BitArray must contain three arrays for beaver triplets"
        );
        let (first, rest) = self.arrays.split_at_mut(1);
        let (second, rest) = rest.split_at_mut(1);
        (&mut first[0], &mut second[0], &mut rest[0])
    }
    /// Unpacks the D2BitArray into three BitArrays, used for beaver triplets.
    #[inline]
    pub fn unpack_as_beaver(&self) -> (&BitArray, &BitArray, &BitArray) {
        assert!(
            self.arrays.len() == 3,
            "D2BitArray must contain at least two arrays for beaver triplets"
        );
        (  &self.arrays[0],  &self.arrays[1],  &self.arrays[2])
    }

    /// XORs the first two arrays of the D2BitArray with the corresponding arrays from the provided beaver triplet.
    #[inline]
    pub fn beaver_xor_d2(&mut self,  beaver_array: &D2BitArray) {
        assert!(
            self.arrays.len() >= 2,
            "D2BitArray must contain at least two arrays for beaver triplets"
        );
        assert!(
            beaver_array.arrays.len() == 2,
            "Incoming beaver array must contain exactly two arrays reconstruction"
        );
        self.arrays[0].inplace_xor(beaver_array.get_array(0));
        self.arrays[1].inplace_xor(beaver_array.get_array(1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BIT_SIZE_CHECK_LIST : [usize; 7] = [
        8, 32, 64, 128, 256, 256+16, 1024
    ];

    #[test]
    fn test_d2array_creation() {
        for array_size in [1, 3, 5] {
            for &bit_size in &BIT_SIZE_CHECK_LIST {
                let a = D2BitArray::from_seed(array_size, bit_size, &[42u8; 32]);
                assert_eq!(a.arrays.len(), array_size, "Array size mismatch");
                for array in &a.arrays {
                    assert_eq!(array.bit_size(), bit_size, "Bit size mismatch");
                }
                
                // check if all arrays are unique
                for i in 0..a.arrays.len() {
                    for j in (i + 1)..a.arrays.len() {
                        assert!(
                            a.arrays[i] != a.arrays[j],
                            "Duplicate BitArray found in D2Array"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn test_d2array_inplace_xor() {
        for array_size in [1, 3, 5] {
            for &bit_size in &BIT_SIZE_CHECK_LIST {
                let mut a = D2BitArray::from_seed(array_size, bit_size, &[42u8; 32]);
                let b = D2BitArray::from_seed(array_size, bit_size, &[13u8; 32]);

                let mut expected = a.clone();
                for (expected_array, other_array) in expected.arrays.iter_mut().zip(&b.arrays) {
                    expected_array.inplace_xor(other_array);
                }

                a.inplace_xor(&b);
                assert_eq!(a, expected, "Mismatch in inplace_xor result");
            }
        }
    }

    #[test]
    fn test_d2array_reconstruction() {
        for array_size in [1, 3, 5] {
            for &bit_size in &BIT_SIZE_CHECK_LIST {
                let share1 = D2BitArray::from_seed(array_size, bit_size, &[42u8; 32]);
                let share2 = D2BitArray::from_seed(array_size, bit_size, &[13u8; 32]);
                let mut expected = share1.clone();
                expected.inplace_xor(&share2);

                let reconstructed = D2BitArray::reconstruct(vec![share1, share2]);
                assert_eq!(reconstructed, expected, "Mismatch in reconstruction result");
            }
        }
    }

    #[test]
    fn test_d2array_push() {
        for array_size in [1, 3, 5] {
            for &bit_size in &BIT_SIZE_CHECK_LIST {
                let mut d2array = D2BitArray::empty();
                for i in 0..array_size {
                    let array = BitArray::from_seed(bit_size, [i as u8; 32]);
                    d2array.push(array.clone());
                    assert_eq!(d2array.arrays.len(), i + 1, "Array size mismatch after push");
                    assert_eq!(d2array.arrays[i], array, "Mismatch in pushed array");
                }
            }
        }
    }

    #[test]
    fn test_d2array_to_slice_additional_cases() {
        for array_size in [1, 3, 5] {
            for &bit_size in &BIT_SIZE_CHECK_LIST {
                let d2array = D2BitArray::from_seed(array_size, bit_size, &[42u8; 32]);
                let byte_size = bit_size.div_ceil(8);

                // Test specific start_byte and end_byte combinations
                let test_cases = vec![
                    (0, byte_size / 2), // First half
                    (byte_size / 4, 3 * byte_size / 4), // Middle section
                    (byte_size / 2, byte_size), // Second half
                ];

                for &(start_byte, end_byte) in &test_cases {

                    let sliced = d2array.to_slice(start_byte, end_byte);

                    assert_eq!(
                        sliced.arrays.len(),
                        d2array.arrays.len(),
                        "Mismatch in number of arrays in sliced D2Array"
                    );

                    for (original, sliced_array) in d2array.arrays.iter().zip(&sliced.arrays) {
                        let expected_data = &original.to_slice(start_byte, end_byte);
                        // let expected_data = &original.data[start_byte..end_byte];
                        assert_eq!(
                            sliced_array, expected_data,
                            "Mismatch in sliced data for range {}..{}",
                            start_byte, end_byte
                        );
                        assert_eq!(
                            sliced_array.bit_size(),
                            (end_byte - start_byte) * 8,
                            "Mismatch in bit size for sliced array"
                        );
                        assert_eq!(
                            sliced_array.byte_size(),
                            end_byte - start_byte,
                            "Mismatch in byte size for sliced array"
                        );
                    }
                }
            }
        }
    }

}

