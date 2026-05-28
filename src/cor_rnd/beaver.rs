use crate::simd_array::{BitArray, D2BitArray};


/// A provider for Beaver triplets, which are used in secure multi-party computation (MPC)
/// Currently, only supports initialize beaver triplets either as a dealer or as a regular party based on seed
pub struct BeaverProvider {
    beaver_storage: Option<D2BitArray>,
    used: usize,
    last_report: usize, //used for more granular material use reporting
}

impl Default for BeaverProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl BeaverProvider {
    pub fn new() -> Self {
        BeaverProvider {
            beaver_storage: None,
            used: 0,
            last_report: 0,
        }
    }

    /// Generate beaver triplets with a given bit number and seed.
    pub fn generate_with_seed(&mut self, bit_num: usize, seed: &[u8; 32]) {
        self.beaver_storage = Some(D2BitArray::from_seed(3, bit_num, seed));
    }

    /// Generates beaver triplets as a dealer
    /// Generates all random shares from other parties and sets dealers share to satisfy a.b=c
    /// Dealer should not be part of the secret shared computation as it knows all shares.
    /// !IMPORTANT Our use of dealer in here is insecure and is intended as a placeholder for benchmarking while deployment should use triplets generated with a maliciously secure approach)
    pub fn generate_as_dealer(
        &mut self,
        bit_num: usize,
        seed: &[u8; 32],
        other_party_seeds: Vec<&[u8; 32]>,
    ) {
        // Using dealers seed to choose random values for the beaver (a, b) values rather than shares.
        // Dealer sets the value of c such that a . b = c without using its seed
        let mut dealer_triplets = D2BitArray::from_seed(2, bit_num, seed);
        // we have to set dealer shares of c manually to make sure beaver relation holds so we start with zero
        dealer_triplets.push(BitArray::new(bit_num));

        let mut recuns = dealer_triplets.clone();
        // Regenerate shares for the other parties via seed
        let other_shares: Vec<D2BitArray> = other_party_seeds
            .iter()
            .map(|other_seed| D2BitArray::from_seed(3, bit_num, other_seed))
            .collect();

        // Reconstruct the shares
        recuns.inplace_reconstruct(other_shares);

        // Compute c value such that : a . b = c
        let c = dealer_triplets.get_mut_array(2);
        c.mut_and(recuns.get_array(0), recuns.get_array(1));
        c.inplace_xor(recuns.get_array(2));

        // Store the dealer's triplets
        self.beaver_storage = Some(dealer_triplets);
    }

    /// Takes 'size' bytes of beaver tiplets from storage.
    /// This function copies the triplets and returns them as a new D2Array.
    /// 
    /// # Panics
    /// Panics if provider is not initialized
    /// Panics if the requested size exceeds the available beaver storage.
    pub fn take(&mut self, req_size: usize) -> D2BitArray {
        let storage = self
            .beaver_storage
            .as_ref()
            .expect("Beaver storage is not initialized");

        log::debug!(
            "Taking {} bytes of beaver triplets from storage(used: {}, inner_byte_size: {}). ",
            req_size,
            self.used,
            storage.inner_byte_size()
        );
        assert!(
            (self.used + req_size) <= storage.inner_byte_size(),
            "Requested size exceeds available beaver storage"
        );
        let result = storage.to_slice(self.used, self.used + req_size);
        self.used += req_size;
        result
    }

    /// report the number of beaver triplet bits used 
    /// Used is measuring per byte so a x8 is necessary for converting to bits
    pub fn report_beaver_usage_then_reset(&mut self) -> usize{
        let out = (self.used-self.last_report)*8;
        self.last_report = self.used;
        out
    }

}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_beaver_generation_with_2_parties() {
        let bit_num = 1024;
        let seed1 = [0u8; 32];
        let seed2 = [1u8; 32];

        let mut provider1 = BeaverProvider::new();
        let mut provider2 = BeaverProvider::new();

        provider1.generate_as_dealer(bit_num, &seed1, vec![&seed2]);
        provider2.generate_with_seed(bit_num, &seed2);

        let reconstructed = D2BitArray::reconstruct(vec![
            provider1.beaver_storage.unwrap(),
            provider2.beaver_storage.unwrap(),
        ]);

        let expected_c = BitArray::and(reconstructed.get_array(0), reconstructed.get_array(1));

        assert_eq!(
            reconstructed.get_array(2),
            &expected_c,
            "Beaver relation does not hold for 2 parties"
        );
    }

    #[test]
    fn test_beaver_generation_with_3_parties() {
        let bit_num = 1024;
        let seed1 = [0u8; 32];
        let seed2 = [1u8; 32];
        let seed3 = [2u8; 32];

        let mut provider1 = BeaverProvider::new();
        let mut provider2 = BeaverProvider::new();
        let mut provider3 = BeaverProvider::new();

        provider1.generate_as_dealer(bit_num, &seed1, vec![&seed2, &seed3]);
        provider2.generate_with_seed(bit_num, &seed2);
        provider3.generate_with_seed(bit_num, &seed3);

        let reconstructed = D2BitArray::reconstruct(vec![
            provider1.beaver_storage.unwrap(),
            provider2.beaver_storage.unwrap(),
            provider3.beaver_storage.unwrap(),
        ]);

        let expected_c = BitArray::and(reconstructed.get_array(0), reconstructed.get_array(1));

        assert_eq!(
            reconstructed.get_array(2),
            &expected_c,
            "Beaver relation does not hold for 2 parties"
        );
    }


}
