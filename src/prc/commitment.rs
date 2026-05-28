use curve25519_dalek::Scalar;
use curve25519_dalek::constants::RISTRETTO_BASEPOINT_TABLE;
use curve25519_dalek::ristretto::{CompressedRistretto, RistrettoBasepointTable, RistrettoPoint};
use once_cell::sync::Lazy;
use sha2::Sha512;

/// Generate a Ristretto generator from domain label
fn generator(label: &[u8]) -> RistrettoPoint {
    RistrettoPoint::hash_from_bytes::<Sha512>(label)
}

// Shared, static base points and tables
// pub static H_RND:RistrettoBasepointTable = RISTRETTO_BASEPOINT_TABLE;

pub static G1T_IDX: Lazy<RistrettoBasepointTable> =
    Lazy::new(|| RistrettoBasepointTable::create(&generator(b"g1_prc_index")));

pub static G2T_RESP: Lazy<RistrettoBasepointTable> =
    Lazy::new(|| RistrettoBasepointTable::create(&generator(b"g2_prc_output")));

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commitment {
    c: RistrettoPoint,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opening {
    rnd: Scalar,
    idx: Scalar,
    resp: Scalar,
}

impl Opening {
    pub fn new(rnd: Scalar, idx: Scalar, resp: Scalar) -> Self {
        Opening { rnd, idx, resp }
    }

    /// Aggregates multiple openings into a single one.
    pub fn aggregate(openings: &[Self]) -> Self {
        let mut agg_opening = openings[0].clone();
        for op in openings.iter().skip(1) {
            agg_opening.rnd += op.rnd;
            agg_opening.idx += op.idx;
            agg_opening.resp += op.resp;
        }
        agg_opening
    }
}

impl Commitment {
    pub fn new(c: RistrettoPoint) -> Self {
        Commitment { c }
    }

    /// commits to an openning via a Pedersen commitment
    pub fn commit(op: &Opening) -> Commitment {
        let c = &op.rnd * RISTRETTO_BASEPOINT_TABLE + &op.idx * &*G1T_IDX + &op.resp * &*G2T_RESP;
        Commitment { c }
    }

    /// Verifies if the commitment matches the opening
    pub fn verify(&self, op: &Opening) -> bool {
        let ground = Self::commit(op);
        self.c == ground.c
    }

    /// Aggregates multiple commitments into a single one.
    pub fn aggregate(commits: &[Self]) -> Self {
        let mut c = RistrettoPoint::default();
        for commit in commits {
            c += commit.c;
        }
        Commitment { c }
    }

    pub fn serialize(&self) -> [u8;32]{
        self.c.compress().to_bytes()
    }
    pub fn deserialize(bytes: &[u8]) -> Self{
        let c = CompressedRistretto(bytes[..32].try_into().unwrap())
            .decompress()
            .expect("Invalid Ristretto point");
        Commitment { c }
    }
}

#[cfg(test)]
mod tests {
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    use super::*;

    #[test]
    fn test_commitment_and_opening() {
        let mut rng = ChaCha8Rng::from_seed([42u8; 32]);
        for _ in 0..10 {
            let rnd = Scalar::random(&mut rng);
            let idx = Scalar::random(&mut rng);
            let resp = Scalar::random(&mut rng);
            let opening = Opening { rnd, idx, resp };
            let commitment = Commitment::commit(&opening);

            assert!(commitment.verify(&opening));
        }
    }

    #[test]
    fn test_commitment_verify_fails_with_wrong_opening() {
        let mut rng = ChaCha8Rng::from_seed([12u8; 32]);
        for _ in 0..10 {
            let rnd = Scalar::random(&mut rng);
            let idx = Scalar::random(&mut rng);
            let resp = Scalar::random(&mut rng);
            let opening = Opening { rnd, idx, resp };
            let commitment = Commitment::commit(&opening);

            // Change one value in opening
            assert!(!commitment.verify(&Opening {
                rnd,
                idx,
                resp: resp + Scalar::ONE
            }));
            assert!(!commitment.verify(&Opening {
                rnd,
                idx: idx + Scalar::ONE,
                resp
            }));
            assert!(!commitment.verify(&Opening {
                rnd: rnd - Scalar::ONE,
                idx,
                resp
            }));
        }
    }

    #[test]
    fn test_aggregate_commitments() {
        let mut rng = ChaCha8Rng::from_seed([4u8; 32]);
        for _ in 0..10 {
            let mut openings = Vec::new();
            let mut commitments = Vec::new();

            for _ in 0..5 {
                let opening = Opening {
                    rnd: Scalar::random(&mut rng),
                    idx: Scalar::random(&mut rng),
                    resp: Scalar::random(&mut rng),
                };
                let commitment = Commitment::commit(&opening);
                openings.push(opening);
                commitments.push(commitment);
            }

            let agg_commitment = Commitment::aggregate(&commitments);

            // Aggregate openings and check if commitment matches
            let agg_opening = Opening::aggregate(&openings);

            assert!(agg_commitment.verify(&agg_opening));
        }
    }

    #[test]
    fn test_generators() {
        let msg = b"test";
        let g1 = generator(msg);
        let h = generator(b"msg2");
        assert_ne!(g1, h);
        let g1_2 = generator(msg);
        assert_eq!(g1_2, g1);
        assert_ne!(g1_2, h);
    }
}
