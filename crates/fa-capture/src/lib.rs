//! ETW event decoding and frame extraction. No `unsafe` allowed here: the C boundary lives in `fa-win::etw`.
#![forbid(unsafe_code)]
