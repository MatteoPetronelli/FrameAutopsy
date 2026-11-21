//! Lock-free structures and the test-only counting allocator.
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::undocumented_unsafe_blocks)]
#![deny(clippy::multiple_unsafe_ops_per_block)]

#[cfg(feature = "test-support")]
pub mod counting;
