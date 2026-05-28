//! utility functions for the prc module

use std::time::Duration;

use libc::{RUSAGE_SELF, getrusage, rusage};
use rand::Rng;
use rand_core::CryptoRngCore;

use crate::cor_rnd::{ArithValueT, BooleanValue};

pub fn get_cpu_time() -> Duration {
    unsafe {
        let mut usage: rusage = std::mem::zeroed();
        if getrusage(RUSAGE_SELF, &mut usage) != 0 {
            panic!("getrusage failed");
        }

        let user = usage.ru_utime;
        let system = usage.ru_stime;

        let secs = user.tv_sec + system.tv_sec;
        let micros = user.tv_usec + system.tv_usec;

        Duration::new(secs as u64, (micros * 1000) as u32)
    }
}

/// Secret share a u32 value into `party_num` shares.
pub fn secret_share_u32<R: Rng>(x: u32, party_num: usize, rng: &mut R) -> Vec<u32> {
    let mut shares: Vec<u32> = (0..(party_num - 1)).map(|_| rng.next_u32()).collect();
    let last_share = shares.iter().fold(x, |acc, &s| acc ^ s);
    shares.push(last_share);
    shares
}

/// Secret share a u32 value into `party_num` shares.
/// Ensures that all shares are l-bit.
///
/// # Panics
/// Panics if `l` is greater than 32.
/// Panics if x does not fit in `l` bits.
pub fn secret_share_lbit<R: Rng>(x: u32, party_num: usize, l: usize, rng: &mut R) -> Vec<u32> {
    assert!(l <= 32, "l must be less than or equal to 32");
    assert!(x < (1 << l), "x must fit in l bits");

    let mask = (1 << l) - 1;
    let mut shares: Vec<u32> = (0..(party_num - 1))
        .map(|_| rng.next_u32() & mask)
        .collect();
    let last_share = shares.iter().fold(x, |acc, &s| acc ^ s);
    shares.push(last_share);
    shares
}

/// Secret share a u32 value into `party_num` shares.
pub fn secret_share_arith<R: CryptoRngCore>(
    x: ArithValueT,
    party_num: usize,
    rng: &mut R,
) -> Vec<ArithValueT> {
    let mut shares: Vec<ArithValueT> = (0..(party_num - 1))
        .map(|_| ArithValueT::random(rng))
        .collect();
    let last_share = shares.iter().fold(x, |mut acc, s| {
        acc.sub(s);
        acc
    });
    shares.push(last_share);
    shares
}

/// Secret share a BooleanValue into `party_num` shares.
pub fn secret_share_boolean<R: Rng>(
    x: BooleanValue,
    party_num: usize,
    rng: &mut R,
) -> Vec<BooleanValue> {
    let mask = (1 << x.bit_len()) - 1;
    let mut shares: Vec<BooleanValue> = (0..(party_num - 1))
        .map(|_| BooleanValue::new(x.bit_len(), rng.next_u32() & mask))
        .collect();

    let last_share_val = shares.iter().fold(x.value(), |acc, s| acc ^ s.value());
    shares.push(BooleanValue::new(x.bit_len(), last_share_val));
    shares
}

pub fn reconstruct_u32(shares: Vec<u32>) -> u32 {
    shares.iter().fold(0, |acc, &s| acc ^ s)
}

pub fn reconstruct_boolean(shares: &[BooleanValue]) -> u32 {
    shares.iter().fold(0, |acc, s| acc ^ s.value())
}

pub fn reconstruct_arith(shares: &[ArithValueT]) -> ArithValueT {
    let mut acc = ArithValueT::zero();
    for s in shares.iter() {
        acc.add(s);
    }
    acc
}

/// Transpose a Vec<Vec<C>> for any type C.
/// Assumes all inner Vecs have the same length.
pub fn transpose<C: Clone>(inp: Vec<Vec<C>>) -> Vec<Vec<C>> {
    if inp.is_empty() || inp[0].is_empty() {
        return Vec::new();
    }
    let row_count = inp.len();
    let col_count = inp[0].len();
    let mut transposed = vec![Vec::with_capacity(row_count); col_count];
    for row in inp {
        for (j, val) in row.into_iter().enumerate() {
            transposed[j].push(val);
        }
    }
    transposed
}
