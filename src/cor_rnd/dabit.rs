use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use rand_core::CryptoRngCore;

use crate::cor_rnd::ArithValueT;


/// Represents a DaBit, a correlated randomness primitive.
/// DaBits represents two sharing of a random value r in z_2 and z_p
/// DaBits can be used for converting Boolean to Arithmetic
#[derive(Debug, Clone)]
pub struct DaBit {
    pub r_z2: bool,
    pub r_zp: ArithValueT, // TODO: Arithmetic domain is set to 2^32 for now, should be set to prime mod 2^255-19
}

impl DaBit {
    pub fn random_dabits<R: CryptoRngCore>(bit_num: usize, rng: &mut R) -> Vec<Self> {
        (0..bit_num)
            .map(|_| DaBit {
                r_z2: (rng.next_u32() % 2) == 1,
                r_zp: ArithValueT::random(rng), // randomness mod M -> now set to 2^32
            })
            .collect()
    }

    /// chooses a random bit value and initializes DaBit with it.
    #[inline]
    fn init_single_bit<R: CryptoRngCore>(rng: &mut R) -> Self {
        let val = rng.next_u32() % 2; // randomly decide the bit value
        DaBit {
            r_z2: val == 1,
            r_zp: ArithValueT::from_u32(val),
        }
    }
    // Perform xor in binary and mod add in arithmetic domain to reconstruct DaBit.
    #[inline]
    pub fn add(&mut self, da: &DaBit) {
        self.r_z2 ^= da.r_z2;
        self.r_zp.add(&da.r_zp);
    }
    #[inline]
    pub fn sub(&mut self, da: &DaBit) {
        self.r_z2 ^= da.r_z2;
        self.r_zp.sub(&da.r_zp);
    }
    pub fn reconstruct(dabits: Vec<DaBit>) -> Self {
        let mut out = dabits[0].clone();
        for da in dabits.iter().skip(1) {
            out.add(da);
        }
        out
    }
}


/// A provider for DaBits, which are correlated randomness used in secure multi-party computation (MPC)
/// Currently, only supports initialize DaBits either as a dealer or as a regular party based on seed
pub struct DaBitProvider {
    da_bit_storage: Option<Vec<DaBit>>,
    used: usize,
    last_report: usize, //used for more granular material use reporting
}

impl Default for DaBitProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl DaBitProvider {
    pub fn new() -> Self {
        DaBitProvider {
            da_bit_storage: None,
            used: 0,
            last_report: 0,
        }
    }

    /// Generate dabits with a seed.
    pub fn generate_with_seed(&mut self, bit_num: usize, seed: &[u8; 32]) {
        let mut rng = ChaCha8Rng::from_seed(*seed);
        self.da_bit_storage = Some(DaBit::random_dabits(bit_num, &mut rng));
        self.used = 0;
        self.last_report = 0;
    }

    /// Generates dabits as a dealer
    /// Generates all random shares from other parties and sets dealers share to satisfy
    /// the relation [r_z2] == [r_zp] <- randin{0,1}
    /// !IMPORTANT Our use of dealer in here is insecure and is intended as a placeholder for
    /// benchmarking while deployment should either run dealer on a semi-honest non-colluding 
    /// party or generate bits via a maliciously secure approach
    pub fn generate_as_dealer(
        &mut self,
        bit_num: usize,
        seed: &[u8; 32],
        other_party_seeds: Vec<&[u8; 32]>,
    ) {
        let mut dealer_rng = ChaCha8Rng::from_seed(*seed);
        let mut dealer_dabit = Vec::with_capacity(bit_num);

        // we have to set dealer shares of c manually to make sure beaver relation holds so we start with zero

        let mut other_shares = Vec::with_capacity(other_party_seeds.len());
        for other_seed in other_party_seeds {
            let mut other_rng = ChaCha8Rng::from_seed(*other_seed);
            other_shares.push(DaBit::random_dabits(bit_num, &mut other_rng));
        }

        for i in 0..bit_num {
            // We set dealer's value so that [r_z2] == [r_zp] <- randin{0,1}
            let mut acc = DaBit::init_single_bit(&mut dealer_rng);
            for other_share in &other_shares {
                acc.sub(&other_share[i]);
            }
            dealer_dabit.push(acc);
        }

        // Store the dealer's triplets
        self.da_bit_storage = Some(dealer_dabit);
        self.used = 0;
        self.last_report = 0;
    }

    /// Returns a vector of req_size fresh daBits.
    /// 
    /// # Panics
    /// Panics if storage is not initialized
    /// Panics if the requested size exceeds the available beaver storage.
    pub fn take(&mut self, req_size: usize) -> Vec<DaBit> {
        let storage = self
            .da_bit_storage
            .as_ref()
            .expect("Beaver storage is not initialized");

        log::debug!(
            "Taking {} dabits from storage(used: {}, total size: {}). ",
            req_size,
            self.used,
            storage.len()
        );

        assert!(
            (self.used + req_size) <= storage.len(),
            "Requested size exceeds available beaver storage"
        );
        let result = storage[self.used..(self.used + req_size)].to_vec();
        self.used += req_size;
        result
    }

    pub fn report_dabit_usage_then_reset(&mut self) -> usize{
        let out = self.used-self.last_report;
        self.last_report = self.used;
        out
    }
}



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dabit_generation_with_2_parties() {
        env_logger::builder()
            .is_test(true) // important: prevents env_logger from clobbering test output
            .try_init()
            .ok();

        let bit_num = 1024;
        let seed1 = [0u8; 32];
        let seed2 = [1u8; 32];

        let mut provider1 = DaBitProvider::new();
        let mut provider2 = DaBitProvider::new();

        provider1.generate_as_dealer(bit_num, &seed1, vec![&seed2]);
        provider2.generate_with_seed(bit_num, &seed2);

        let dabits1 = provider1.take(bit_num);
        let dabits2 = provider2.take(bit_num);

        let rec: Vec<DaBit> = dabits1
            .iter()
            .zip(dabits2.iter())
            .map(|(d1, d2)| {
                let mut reconstructed = d1.clone();
                reconstructed.add(d2);
                reconstructed
            })
            .collect();

        for i in 0..5 {
            log::debug!("DaBit {}:   d1: {:?}", i, dabits1[i],);
            log::debug!("DaBit {}:   d2: {:?}", i, dabits1[i],);
            log::debug!("DaBit {}:  rec: {:?}", i, rec[i],);
        }

        for (i, r) in rec.iter().enumerate() {
            assert_eq!(
                r.r_z2 as u32, r.r_zp.as_u32(),
                "DaBit relation does not hold for 2 parties at index {}",
                i
            );
        }
    }

}
