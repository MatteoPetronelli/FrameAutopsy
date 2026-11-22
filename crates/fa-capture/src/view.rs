//! The borrowed view of one ETW event, as the callback hands it over.

/// GUIDs as `u128` in the order `data1:data2:data3:data4` read as hex (so `0x22fb2cd6_0e7b_422b_a0c7_2fad1fd0e716`).
pub mod guids {
    pub const DXGI: u128 = 0xca11c036_0102_4a2d_a6ad_f03cfed5d3c9;
    pub const DXGKRNL: u128 = 0x802ec45a_1e99_4b83_9920_87c98277ba9d;
    pub const KERNEL_PROCESS: u128 = 0x22fb2cd6_0e7b_422b_a0c7_2fad1fd0e716;
    /// Classic kernel classes (the "MSNT_SystemTrace" MOF events).
    pub const PERFINFO: u128 = 0xce1dbfb4_137e_4da6_87b0_3f59aa102cbc;
    pub const IMAGE: u128 = 0x2cb15d1d_5fc1_11d2_abe1_00a0c911f518;
    pub const THREAD: u128 = 0x3d6fa8d1_fe05_11d0_9dda_00c04fd7ba7c;
    pub const DISKIO: u128 = 0x3d6fa8d4_fe05_11d0_9dda_00c04fd7ba7c;
    /// Classic process events: their image name is truncated to 15 bytes, so they are never a name source.
    pub const NT_PROCESS: u128 = 0x3d6fa8d0_fe05_11d0_9dda_00c04fd7ba7c;
}

/// One event. `data` is valid only for the duration of the callback: copy what you need, never store the view.
#[derive(Clone, Copy, Debug)]
pub struct EventView<'a> {
    pub provider: u128,
    pub id: u16,
    pub version: u8,
    pub opcode: u8,
    /// Raw TSC ticks (`ClientContext = 3` and `PROCESS_TRACE_MODE_RAW_TIMESTAMP`).
    pub timestamp: u64,
    pub pid: u32,
    pub tid: u32,
    pub cpu: u16,
    /// The event was written by a 32-bit process: pointers in `data` are 4 bytes.
    pub ptr32: bool,
    pub data: &'a [u8],
}
