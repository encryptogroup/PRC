use crate::{
    cor_rnd::{ArithValueT, BooleanValue, DaBitProvider},
    prc::connection::MpcMessageHandler,
    simd_array::BitArray,
};

pub struct ConvLayer {
    // party_const determines if this party handles const additions in mpc
    party_const: bool,
}

impl ConvLayer {
    pub fn new(party_const: bool) -> Self {
        ConvLayer { party_const }
    }

    /// Interact with other MPC parties to convert the boolean secret shares to arithmetic shares.
    pub async fn interact(
        &mut self,
        boolean_values: Vec<BooleanValue>,
        dabit_provider: &mut DaBitProvider,
        message_handler: &mut MpcMessageHandler,
    ) -> Vec<ArithValueT> {
        let total_bits = boolean_values
            .iter()
            .map(|b| b.bit_len() as usize)
            .sum::<usize>();

        // Decompose to bits
        let mut b = BitArray::new(total_bits);
        let mut current = 0;
        for bv in boolean_values.iter() {
            for i in 0..bv.bit_len() {
                b.set_bit(current, (bv.value() >> i) & 1 != 0);
                current += 1;
            }
        }
        log::debug!(
            "Running run_conv_B2A on party {}\n  input bits: {:?}",
            self.party_const,
            b
        );

        let dabits = dabit_provider.take(total_bits);
        let mut e = BitArray::new(total_bits);
        for (i, dabit) in dabits.iter().enumerate() {
            e.set_bit(i, b.get(i) ^ dabit.r_z2);
        }
        log::debug!(
            "Blinded dabits on party {} -> e:{:?}\n  dabits bits: {:?}",
            self.party_const,
            e,
            dabits
        );

        log::debug!("Running send_dabit_shares on party {}", self.party_const);
        message_handler.send_dabit_shares(e.clone()).await;

        let rec_shares = message_handler.receive_dabit_shares().await;
        log::debug!("Received dabit_shares on party {}", self.party_const);
        e.inplace_reconstruct(&rec_shares);
        log::debug!(
            "reconstructed dabits on party {} -> e:{:?}\n",
            self.party_const,
            e
        );

        let mut arith = Vec::with_capacity(boolean_values.len());
        let mut current = 0;
        for bv in boolean_values {
            let mut val = ArithValueT::zero();
            for i in 0..bv.bit_len() {
                // bit_A = e + [r_A] - 2e[r_A]
                let mut round = dabits[current].r_zp;
                let mut e_ra_mul2 = dabits[current].r_zp;
                e_ra_mul2.mul(&ArithValueT::from_u32(2 * e.get(current) as u32));
                round.sub(&e_ra_mul2);

                if self.party_const {
                    // if this party is the constant party, we add the const e
                    round.add(&ArithValueT::from_u32(e.get(current) as u32));
                }
                round.mul(&ArithValueT::from_u32(1u32 << i));
                val.add(&round);
                current += 1;
            }
            arith.push(val);
        }
        arith
    }
}
