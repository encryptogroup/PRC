//! provides correlated randomness: beaver triplets and dabits
mod beaver;
mod dabit;
mod sharing;

pub use beaver::BeaverProvider;
pub use dabit::{DaBitProvider, DaBit};
pub use sharing::{ArithValueT, BooleanValue, join_boolean};