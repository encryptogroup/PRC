//! A library for efficient SIMD bit array operations
//! Our focus is on providing a high-performance implementation of bit arrays for MPC (Multi-Party 
//! Computation) protocols. We aim to optimize operations like AND, XOR, and NOT using SIMD 
//! (Single Instruction, Multiple Data) techniques and prioritize array-based bitwise operations 
//! over flexibility or efficiency for handling non--aligned bit operations.
mod aligned_array;
mod bit_array;
mod d2_bit_array;

pub use bit_array::BitArray;
pub use d2_bit_array::D2BitArray;
