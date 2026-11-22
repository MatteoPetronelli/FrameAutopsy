//! ETW event decoding and frame extraction. No `unsafe` allowed here: the C boundary lives in `fa-win::etw`.
//!
//! Events reach this crate as a borrowed [`view::EventView`] (header fields + the user-data bytes, valid only during
//! the callback). [`decode::decode`] turns the hot ones into fixed-size `Copy` records with hand-written layouts keyed
//! by provider, event id and version; [`capture::Capture`] pushes them into lock-free rings without allocating.
#![forbid(unsafe_code)]

pub mod capture;
pub mod decode;
pub mod modules;
pub mod processes;
pub mod record;
pub mod synthetic;
pub mod view;
