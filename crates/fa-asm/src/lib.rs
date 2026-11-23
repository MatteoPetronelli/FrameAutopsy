//! asm/SIMD kernels, each earning its place in docs/asm-unsafe-catalog.md.
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::undocumented_unsafe_blocks)]
#![deny(clippy::multiple_unsafe_ops_per_block)]

pub mod bw;
pub mod chase;
pub mod cpuid;
pub mod fma;
pub mod freq;
pub mod tsc;
