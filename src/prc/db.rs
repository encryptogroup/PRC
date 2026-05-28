use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

use crate::{prc::config::DBConfig, simd_array::{BitArray, D2BitArray}};

// use super::server::SystemConfig;

/// Represents a database that stores records for PRC.
/// This database have an unconventional structure, where *different bits of a record are stored
/// separately* in different bit_dbs.
/// The bit_dbs[l] stores the l'th bit of record_idx in bit_dbs[l][idx/column_num][idx%column_num]
/// bit_dbs[0] is the LSB
///
/// Expects row_num to be a multiple of 8, and record_size to be at most 32 bits.
/// Expects row_num to be larger than column_num.
/// Expects record_size to be at least 1 bit and at most 32 bits.
pub struct PRCDatabase {
    bit_dbs: Vec<D2BitArray>, // bit_dbs[i] stores the i'th bit of each record

    record_size: usize,
    l2_dim: usize, // Corresponds to BitArray in D2BitArray
    l1_dim: usize, // Correspond to bit in Bitarray
}

impl PRCDatabase {
    /// Creates a (random) PRCDatabase from a seed.
    ///
    /// # Panics
    /// Panics if record_size is larger than 32 bits.
    /// Panics if dimensions are not a power of two.
    /// Panics if l1_dim is smaller than l2_dim.
    /// Panics if l1_dim is smaller than 8.
    pub fn from_seed(record_size: usize, l2_dim: usize, l1_dim: usize, seed: &[u8; 32]) -> Self {
        assert!(l1_dim.is_power_of_two(), "Dimensions (l1_dim) must be powers of 2");
        assert!(l2_dim.is_power_of_two(), "Dimensions (l2_dim) must be powers of 2");
        assert!(l1_dim >= l2_dim , "Expected dimension l1 to be larger or equal l2");
        assert!(l1_dim >= 8, "Expected dimension l1 to be at least 8");
        assert!(record_size <= 32 , "Record size must be at most 32 bits");

        let mut rng = ChaCha8Rng::from_seed(*seed);

        let mut bit_dbs = Vec::with_capacity(record_size);
        for _ in 0..record_size {
            bit_dbs.push(D2BitArray::random(l2_dim, l1_dim, &mut rng));
        }
        PRCDatabase {
            bit_dbs,
            record_size,
            l2_dim,
            l1_dim,
        }
    }

    /// Creates a (random) PRCDatabase from a given [SystemConfig].
    pub fn from_config(config: &DBConfig) -> Self {
        Self::from_seed(
            config.db_record_size(), // record size
            config.l2_dim(),      // column number
            config.l1_dim(),      // row number
            config.db_seed(),
        )
    }

    // /// Gets the DB dimensions
    // pub fn get_dims(&self) -> (usize, usize) {
    //     (self.l1_dim, self.l2_dim)
    // }

    /// Converts a index to a 2D index (l1, l2).
    ///
    /// # Panics
    /// Panics if the index is larger than total size.
    #[inline]
    fn idx_to_2dim(&self, idx: usize) -> (usize, usize) {
        assert!(idx < self.total_size(), "Index out of bounds");
        let l2 = idx / self.l1_dim;
        let l1 = idx % self.l1_dim;
        (l1, l2)
    }

    /// Gets the {bit_choice} bit of the record at index {idx}.
    ///
    /// # Panics
    /// Panics if the index is larger than total size or if bit_choice is larger than record size.
    #[inline]
    pub fn get_record_bit(&self, idx: usize, bit_choice: usize) -> bool {
        assert!(idx < self.total_size(), "Index out of bounds");
        assert!(
            bit_choice < self.record_size,
            "Bit choice is larger than record size"
        );
        let (l1, l2) = self.idx_to_2dim(idx);
        log::debug!(
            "Getting data bit for idx: {}, bit_choice: {}, l1: {}, l2: {}",
            idx,
            bit_choice,
            l1,
            l2
        );
        self.bit_dbs[bit_choice].get_array(l2).get(l1)
    }

    /// Gets the the record at index {idx}.
    /// Records start at index 0, and are of size {record_size} bits.
    ///
    /// # Panics
    /// Panics if the index is larger than total size.
    pub fn get_record(&self, idx: usize) -> u32 {
        let mut out = 0;
        let mut pw = 1;
        for i in 0..self.record_size {
            out += pw * (self.get_record_bit(idx, i) as u32);
            pw *= 2;
        }
        out
    }

    /// Gets the number of bits necessary to present database dimensions 
    #[inline]
    pub fn get_logdim(&self) -> (usize, usize) {
        let log_l1 = self.l1_dim.trailing_zeros() as usize;
        let log_l2 = self.l2_dim.trailing_zeros() as usize;
        (log_l1, log_l2)
    }
    /// Gets the db dimensions 
    #[inline]
    pub fn get_dims(&self) -> (usize, usize) {
        (self.l1_dim, self.l2_dim)
    }

    /// Maximum number of records in the database.
    #[inline]
    pub fn total_size(&self) -> usize {
        self.l1_dim * self.l2_dim
    }

    /// Performs lvl1 PIR retrieval with a clear database and secret shared OHE (One-Hot Encoding)
    /// representing the l1 index of the record to retrieve.
    /// 
    /// At the moment does not support multi-bit records.
    /// 
    /// # Panics
    /// Panics if the record size is larger than 1 bit. 
    /// Panics if the OHE size does not match l1_dimension.
    pub fn lvl1_pir_retrieve(&self, ohe: &BitArray) -> BitArray {
        if self.record_size > 1 {
            todo!("Level 1 retrieval is only supported for single bit records");
        }
        if ohe.bit_size() != self.l1_dim {
            panic!("The lvl1 query OHE size does not match the row number");
        }
        let mut out = BitArray::new(self.l2_dim);

        for i in 0..self.l2_dim {
            let res = BitArray::inner_prod(ohe, self.bit_dbs[0].get_array(i));
            out.set_bit(i, res);
        }

        out
    }

    /// Provides a plaintext lvl1 retrieval for testing purposes.
    pub fn _lvl1_plain_idx(&self, idx_lv_1: usize) -> BitArray {
        let mut out = BitArray::new(self.l2_dim);
        for i in 0..self.l2_dim {
            out.set_bit(i, self.bit_dbs[0].get_array(i).get(idx_lv_1));
        }
        out
    }
}


#[cfg(test)]
mod tests {
    use rand::RngCore;

    use super::*;

    #[test]
    fn test_lvl1_pir_retrieve() {
        let record_size = 1; // Single bit records
        let column_num = 64; // 
        let row_num = 8192; // 8k
        let base_seed = [42u8; 32]; // Fixed seed for reproducibility

        // Create a fixed database
        let db1 = PRCDatabase::from_seed(record_size, column_num, row_num, &base_seed);
        let db2 = PRCDatabase::from_seed(record_size, column_num, row_num, &base_seed);

        assert_eq!(
            db1.bit_dbs.first(),
            db2.bit_dbs.first(),
            "Databases should be identical when initialized with the same seed"
        );

        // Choose random indexes to retrieve
        let mut rng = ChaCha8Rng::from_seed([0u8;32]);
        let mut random_indexes = Vec::new();
        for _ in 0..100 {
            random_indexes.push((rng.next_u32() as usize) % row_num);
        }

        // Perform retrieval for each random index
        for idx in random_indexes {
            log::debug!("Starting a retrieval lvl1 for index: {}", idx);
            let mut ohe = BitArray::new(row_num);
            ohe.set_bit(idx % row_num, true); // Create a one-hot encoded vector for the index

            // secret share ohe
            let q1 = BitArray::from_seed(row_num, [13u8; 32]);
            let mut q2 = q1.clone();
            q2.inplace_xor(&ohe);

            let result_ss1 = db1.lvl1_pir_retrieve(&q1);
            let result_ss2 = db2.lvl1_pir_retrieve(&q2);
            let result = BitArray::xor(&result_ss1, &result_ss2);

            for i in 0..column_num {
                let expected = db1.get_record_bit(i * row_num + idx, 0);
                assert_eq!(result.get(i), expected, "Mismatch at index {}", i);
            }
        }
    }
}
