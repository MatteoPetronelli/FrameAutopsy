//! Windows FFI. Public API is safe.
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::undocumented_unsafe_blocks)]
#![deny(clippy::multiple_unsafe_ops_per_block)]

pub mod clock;
pub mod console;
pub mod etw;
pub mod labsys;
pub mod mem;
pub mod pdh;
pub mod power;
pub mod procs;
pub mod render;
pub mod thread;
pub mod timer;
pub mod topology;
pub mod windows;
