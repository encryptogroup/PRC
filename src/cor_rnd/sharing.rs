use curve25519_dalek::scalar::Scalar;
use rand::Rng;
use rand_core::CryptoRngCore;
use std::fmt;

/// Choose what type of arithmetic value to use.
/// Currently, we use u32 as the arithmetic domain, which is a wrapping operation with no extra cost.
pub type ArithValueT = ArithValue<Scalar>;

/// Represents an arithmetic secret shared value in a secure multi-party computation (MPC) context.
/// Currently, only supporting mod 2^32, which is a wrapping operation with no cost.
#[derive(Debug, Clone, Copy)]
pub struct ArithValue<T> {
    value: T,
}

/// Creates a new arithmetic sharing mod 2^32 where mod done via wrapping operations at no cost
impl ArithValue<u32> {
    /// Create a zero value
    #[inline]
    pub fn zero() -> Self {
        ArithValue { value: 0u32 }
    }
    /// Create an arithmetic value from a u32
    #[inline]
    pub fn from_u32(value: u32) -> Self {
        ArithValue { value }
    }
    /// generate a random arithmetic value
    #[inline]
    pub fn random<R: Rng>(rng: &mut R) -> Self {
        let value = rng.next_u32();
        ArithValue { value }
    }
    /// get the value as u32
    #[inline]
    pub fn as_u32(&self) -> u32 {
        self.value
    }
    /// modular addition
    #[inline]
    pub fn add(&mut self, other: &ArithValue<u32>) {
        self.value = self.value.wrapping_add(other.value);
    }
    /// modular subtraction
    #[inline]
    pub fn sub(&mut self, other: &ArithValue<u32>) {
        self.value = self.value.wrapping_sub(other.value);
    }
    /// modlar multiplication
    #[inline]
    pub fn mul(&mut self, other: &ArithValue<u32>) {
        self.value = self.value.wrapping_mul(other.value);
    }
}

/// Creates a new arithmetic sharing compatible with Pedersen over ECC
impl ArithValue<Scalar> {
    /// Create a zero value
    #[inline]
    pub fn zero() -> Self {
        ArithValue {
            value: Scalar::ZERO,
        }
    }
    /// Create an arithmetic value from a u32
    #[inline]
    pub fn from_u32(value: u32) -> Self {
        ArithValue {
            value: Scalar::from(value),
        }
    }
    /// generate a random arithmetic value
    #[inline]
    pub fn random<R: CryptoRngCore>(rng: &mut R) -> Self {
        ArithValue {
            value: Scalar::random(rng),
        }
    }

    /// get the value as u32
    /// Expects that the value is in the range of u32 (this is not true for ECC points).
    /// # Panics
    /// Panics if the value is larger than u32::MAX.
    #[inline]
    pub fn as_u32(&self) -> u32 {
        let bytes = self.value.to_bytes(); // 32-byte little-endian
        let value = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);

        // Assert that the upper bytes are zero (i.e., value fits in u32)
        assert!(
            bytes[4..].iter().all(|&b| b == 0),
            "Scalar value does not fit in u32"
        );
        value
    }
    /// get the scalar value
    #[inline]
    pub fn as_scalar(&self) -> Scalar {
        self.value
    }

    /// modular addition
    #[inline]
    pub fn add(&mut self, other: &ArithValue<Scalar>) {
        self.value += other.value;
    }
    /// modular subtraction
    #[inline]
    pub fn sub(&mut self, other: &ArithValue<Scalar>) {
        self.value -= other.value;
    }
    /// modlar multiplication
    #[inline]
    pub fn mul(&mut self, other: &ArithValue<Scalar>) {
        self.value *= other.value;
    }
}

/// Supports conversion between binary and arithmetic representations in MPC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BooleanValue {
    bit_len: u8,
    value: u32,
}

impl BooleanValue {
    pub fn new(bit_len: u8, value: u32) -> Self {
        if bit_len > 32 {
            panic!("Cannot represent a value with more than 32 bits in a u32");
        }
        if bit_len < 32 && (value >= (1 << bit_len)) {
            panic!("Value {} exceeds bit length {}", value, bit_len);
        }
        BooleanValue { bit_len, value }
    }

    #[inline]
    pub fn bit_len(&self) -> u8 {
        self.bit_len
    }

    #[inline]
    pub fn value(&self) -> u32 {
        self.value
    }
}

/// takes two BooleanValues and appends them together to form a new BooleanValue.
/// given l and h, creates a value of (l||h) with l.bit_len + h.bit_len bits
pub fn join_boolean(lower: BooleanValue, higher: BooleanValue) -> BooleanValue {
    let value = (higher.value() << lower.bit_len()) | lower.value();
    BooleanValue::new(lower.bit_len() + higher.bit_len(), value)
}

impl fmt::Display for BooleanValue {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.value)
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arithvalue_u32_add_sub_mul() {
        let mut a = ArithValue::<u32>::from_u32(10);
        let b = ArithValue::<u32>::from_u32(20);
        a.add(&b);
        assert_eq!(a.as_u32(), 30);

        a.sub(&ArithValue::<u32>::from_u32(5));
        assert_eq!(a.as_u32(), 25);

        a.mul(&ArithValue::<u32>::from_u32(2));
        assert_eq!(a.as_u32(), 50);
    }

    #[test]
    #[should_panic(expected = "Scalar value does not fit in u32")]
    fn test_arithvalue_scalar_to_u32_panic() {
        let big_scalar = Scalar::from(1u64 << 40);
        let v = ArithValue::<Scalar> { value: big_scalar };
        let _ = v.as_u32();
    }

    #[test]
    fn test_arithvalue_scalar_add_sub_mul() {
        let mut a = ArithValue::<Scalar>::from_u32(10);
        let b = ArithValue::<Scalar>::from_u32(20);
        a.add(&b);
        assert_eq!(a.as_u32(), 30);

        a.sub(&ArithValue::<Scalar>::from_u32(5));
        assert_eq!(a.as_u32(), 25);

        a.mul(&ArithValue::<Scalar>::from_u32(2));
        assert_eq!(a.as_u32(), 50);
    }


    #[test]
    #[should_panic]
    fn test_boolean_value_bit_len_too_large() {
        BooleanValue::new(33, 1);
    }

    #[test]
    #[should_panic]
    fn test_boolean_value_value_exceeds_bit_len() {
        BooleanValue::new(4, 20);
    }

    #[test]
    fn test_append_boolean() {
        let l = BooleanValue::new(4, 0b1010);
        let h = BooleanValue::new(4, 0b1100);
        let appended = join_boolean(l, h);
        assert_eq!(appended.bit_len(), 8);
        assert_eq!(appended.value(), 0b11001010);
    }
}
