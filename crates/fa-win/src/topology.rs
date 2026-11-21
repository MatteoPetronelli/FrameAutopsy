//! WIN-03: CPU topology from Windows (`GetSystemCpuSetInformation`, `GetLogicalProcessorInformationEx`).
//!
//! The parsers take plain `&[u8]` and are safe code, so synthetic buffers (hybrid, dual-CCD, malformed) feed the same
//! path as the real call. Every record `Size` is bounds-checked first: a bad buffer is an error, never a panic, an
//! out-of-bounds read or an endless loop. Only `read()` contains `unsafe`.
//!
//! Field offsets follow the Windows SDK layouts (checked against `windows-sys` in the tests).
//! shortcut: processor groups are modelled (`group`) but pinning and cache masks assume one group (<= 64 CPUs).

use std::fmt;
use std::io;
use windows_sys::Win32::Foundation::ERROR_INSUFFICIENT_BUFFER;
use windows_sys::Win32::System::SystemInformation::{
    GetLogicalProcessorInformationEx, GetSystemCpuSetInformation, RelationCache,
};
use windows_sys::Win32::System::Threading::GetCurrentProcess;

// SYSTEM_CPU_SET_INFORMATION: Size u32 @0, Type u32 @4, then the CpuSet union @8.
const CPU_SET_TYPE: usize = 4;
const CPU_SET_ID: usize = 8;
const CPU_SET_GROUP: usize = 12;
const CPU_SET_LOGICAL_INDEX: usize = 14;
const CPU_SET_CORE: usize = 15;
const CPU_SET_LLC: usize = 16;
const CPU_SET_NUMA: usize = 17;
const CPU_SET_EFFICIENCY: usize = 18;
const CPU_SET_MIN_SIZE: usize = 19;
const CPU_SET_INFORMATION: u32 = 0; // CpuSetInformation

// SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX: Relationship u32 @0, Size u32 @4, then the union @8 (CACHE_RELATIONSHIP).
const EX_RELATIONSHIP: usize = 0;
const CACHE_LEVEL: usize = 8;
const CACHE_SIZE: usize = 12;
const CACHE_TYPE: usize = 16;
const CACHE_GROUP_MASK: usize = 40;
const CACHE_MIN_SIZE: usize = 48;
const RELATION_CACHE: u32 = 2; // RelationCache

#[derive(Debug)]
pub enum TopologyError {
    /// A record `Size` is zero, smaller than its header, or runs past the buffer.
    BadRecord { offset: usize, size: u32 },
    /// A cache record carries an unknown `PROCESSOR_CACHE_TYPE`.
    UnknownCacheType(i32),
    /// The Windows call failed (raw OS error).
    Os(i32),
}

impl fmt::Display for TopologyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadRecord { offset, size } => {
                write!(f, "bad topology record at byte {offset} (size {size})")
            }
            Self::UnknownCacheType(t) => write!(f, "unknown cache type {t}"),
            Self::Os(e) => write!(
                f,
                "topology call failed: {}",
                io::Error::from_raw_os_error(*e)
            ),
        }
    }
}

impl std::error::Error for TopologyError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LogicalCpu {
    /// `LogicalProcessorIndex`, within `group`.
    pub index: u32,
    pub cpu_set_id: u32,
    pub core: u32,
    pub llc: u32,
    /// Higher is faster (P cores above E cores).
    pub efficiency_class: u8,
    pub numa_node: u32,
    pub group: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CacheKind {
    Unified,
    Instruction,
    Data,
    Trace,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cache {
    pub level: u8,
    pub size_bytes: u32,
    pub kind: CacheKind,
    pub group_mask: usize,
}

#[derive(Clone, Debug, Default)]
pub struct Topology {
    pub cpus: Vec<LogicalCpu>,
    pub caches: Vec<Cache>,
}

fn distinct(values: impl Iterator<Item = u32>) -> usize {
    let mut v: Vec<u32> = values.collect();
    v.sort_unstable();
    v.dedup();
    v.len()
}

impl Topology {
    pub fn physical_cores(&self) -> usize {
        distinct(self.cpus.iter().map(|c| c.core))
    }

    pub fn llc_domains(&self) -> usize {
        distinct(self.cpus.iter().map(|c| c.llc))
    }

    pub fn is_hybrid(&self) -> bool {
        distinct(self.cpus.iter().map(|c| u32::from(c.efficiency_class))) > 1
    }
}

/// Callers guarantee `at + 4 <= buf.len()` (record length checked in `for_each_record`).
fn u32_at(buf: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([buf[at], buf[at + 1], buf[at + 2], buf[at + 3]])
}

fn u16_at(buf: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([buf[at], buf[at + 1]])
}

/// Splits a buffer of variable-size records (`Size` is a u32 at `size_at`) and hands each, bounds-checked, to `f`.
/// `min_size` is the smallest record that may be read at all; `f` re-checks what it reads beyond it.
fn for_each_record<T>(
    buf: &[u8],
    size_at: usize,
    min_size: usize,
    mut f: impl FnMut(&[u8]) -> Result<Option<T>, TopologyError>,
) -> Result<Vec<T>, TopologyError> {
    let mut out = Vec::new();
    let mut offset = 0usize;
    while offset < buf.len() {
        let rest = &buf[offset..];
        let bad = |size| TopologyError::BadRecord { offset, size };
        if rest.len() < size_at + 4 {
            return Err(bad(0));
        }
        let size = u32_at(rest, size_at);
        let len = size as usize;
        if len < min_size || len > rest.len() {
            return Err(bad(size));
        }
        out.extend(f(&rest[..len])?);
        offset += len;
    }
    Ok(out)
}

/// Parses a `SYSTEM_CPU_SET_INFORMATION` record array; records that are not `CpuSetInformation` are skipped.
pub fn parse_cpu_sets(buf: &[u8]) -> Result<Vec<LogicalCpu>, TopologyError> {
    for_each_record(buf, 0, CPU_SET_TYPE + 4, |r| {
        if u32_at(r, CPU_SET_TYPE) != CPU_SET_INFORMATION {
            return Ok(None);
        }
        if r.len() < CPU_SET_MIN_SIZE {
            return Err(TopologyError::BadRecord {
                offset: 0,
                size: r.len() as u32,
            });
        }
        Ok(Some(LogicalCpu {
            index: u32::from(r[CPU_SET_LOGICAL_INDEX]),
            cpu_set_id: u32_at(r, CPU_SET_ID),
            core: u32::from(r[CPU_SET_CORE]),
            llc: u32::from(r[CPU_SET_LLC]),
            efficiency_class: r[CPU_SET_EFFICIENCY],
            numa_node: u32::from(r[CPU_SET_NUMA]),
            group: u16_at(r, CPU_SET_GROUP),
        }))
    })
}

/// Parses a `SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX` record array; only `RelationCache` records are kept.
pub fn parse_caches(buf: &[u8]) -> Result<Vec<Cache>, TopologyError> {
    for_each_record(buf, 4, 8, |r| {
        if u32_at(r, EX_RELATIONSHIP) != RELATION_CACHE {
            return Ok(None);
        }
        if r.len() < CACHE_MIN_SIZE {
            return Err(TopologyError::BadRecord {
                offset: 0,
                size: r.len() as u32,
            });
        }
        let kind = match u32_at(r, CACHE_TYPE) as i32 {
            0 => CacheKind::Unified,
            1 => CacheKind::Instruction,
            2 => CacheKind::Data,
            3 => CacheKind::Trace,
            t => return Err(TopologyError::UnknownCacheType(t)),
        };
        let mask = u64::from(u32_at(r, CACHE_GROUP_MASK))
            | (u64::from(u32_at(r, CACHE_GROUP_MASK + 4)) << 32);
        Ok(Some(Cache {
            level: r[CACHE_LEVEL],
            size_bytes: u32_at(r, CACHE_SIZE),
            kind,
            group_mask: mask as usize,
        }))
    })
}

/// Runs a "size query then fill" Windows call. `call(ptr, len)` gets a buffer of `*len` bytes (null when 0) and returns
/// success; `*len` is the capacity in, the needed or written size out. The buffer is `u64`-backed for alignment.
fn fetch(mut call: impl FnMut(*mut u8, &mut u32) -> bool) -> Result<Vec<u8>, TopologyError> {
    let mut len = 0u32;
    for _ in 0..4 {
        let mut words = vec![0u64; (len as usize).div_ceil(8)];
        let ptr = if len == 0 {
            std::ptr::null_mut()
        } else {
            words.as_mut_ptr().cast::<u8>()
        };
        if call(ptr, &mut len) {
            return Ok(words
                .iter()
                .flat_map(|w| w.to_ne_bytes())
                .take(len as usize)
                .collect());
        }
        let err = io::Error::last_os_error().raw_os_error().unwrap_or(0);
        if err != ERROR_INSUFFICIENT_BUFFER as i32 {
            return Err(TopologyError::Os(err));
        }
    }
    // The size kept changing under us (hot-plug); give up rather than loop.
    Err(TopologyError::Os(ERROR_INSUFFICIENT_BUFFER as i32))
}

/// Reads the live topology from Windows.
pub fn read() -> Result<Topology, TopologyError> {
    let cpu_sets = fetch(|ptr, len| {
        // SAFETY: `GetCurrentProcess` has no preconditions; it returns a pseudo-handle, not a handle to open or close.
        let me = unsafe { GetCurrentProcess() };
        let capacity = *len;
        // SAFETY: `ptr` is null with `capacity` 0 or points to `capacity` writable, 8-aligned bytes; `len` is a valid
        // u32 for the returned length; flags 0 asks for the default CPU sets.
        unsafe { GetSystemCpuSetInformation(ptr.cast(), capacity, len, me, 0) != 0 }
    })?;
    let caches = fetch(|ptr, len| {
        // SAFETY: `ptr` is null with `*len` 0 or points to `*len` writable, 8-aligned bytes; `len` is the valid in/out
        // u32 the API requires.
        unsafe { GetLogicalProcessorInformationEx(RelationCache, ptr.cast(), len) != 0 }
    })?;
    Ok(Topology {
        cpus: parse_cpu_sets(&cpu_sets)?,
        caches: parse_caches(&caches)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::offset_of;
    use windows_sys::Win32::System::SystemInformation::{
        CACHE_RELATIONSHIP, SYSTEM_CPU_SET_INFORMATION, SYSTEM_CPU_SET_INFORMATION_0_0,
        SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX,
    };

    /// Hex fixture: whitespace separated bytes, `#` starts a comment.
    fn hex(text: &str) -> Vec<u8> {
        text.lines()
            .flat_map(|l| l.split('#').next().unwrap_or("").split_whitespace())
            .map(|b| u8::from_str_radix(b, 16).expect("hex byte"))
            .collect()
    }

    fn fixture(name: &str) -> Vec<u8> {
        let path = format!(
            "{}/../../lab/fixtures/topology/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        hex(&std::fs::read_to_string(path).expect("fixture"))
    }

    #[test]
    fn offsets_match_the_windows_sdk_structs() {
        let cs = offset_of!(SYSTEM_CPU_SET_INFORMATION, Anonymous);
        let set = |f: usize| cs + f;
        assert_eq!(offset_of!(SYSTEM_CPU_SET_INFORMATION, Type), CPU_SET_TYPE);
        assert_eq!(
            set(offset_of!(SYSTEM_CPU_SET_INFORMATION_0_0, Id)),
            CPU_SET_ID
        );
        assert_eq!(
            set(offset_of!(SYSTEM_CPU_SET_INFORMATION_0_0, Group)),
            CPU_SET_GROUP
        );
        assert_eq!(
            set(offset_of!(
                SYSTEM_CPU_SET_INFORMATION_0_0,
                LogicalProcessorIndex
            )),
            CPU_SET_LOGICAL_INDEX
        );
        assert_eq!(
            set(offset_of!(SYSTEM_CPU_SET_INFORMATION_0_0, CoreIndex)),
            CPU_SET_CORE
        );
        assert_eq!(
            set(offset_of!(
                SYSTEM_CPU_SET_INFORMATION_0_0,
                LastLevelCacheIndex
            )),
            CPU_SET_LLC
        );
        assert_eq!(
            set(offset_of!(SYSTEM_CPU_SET_INFORMATION_0_0, NumaNodeIndex)),
            CPU_SET_NUMA
        );
        assert_eq!(
            set(offset_of!(SYSTEM_CPU_SET_INFORMATION_0_0, EfficiencyClass)),
            CPU_SET_EFFICIENCY
        );

        let ex = offset_of!(SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX, Anonymous);
        let c = |f: usize| ex + f;
        assert_eq!(
            offset_of!(SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX, Relationship),
            EX_RELATIONSHIP
        );
        assert_eq!(offset_of!(SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX, Size), 4);
        assert_eq!(c(offset_of!(CACHE_RELATIONSHIP, Level)), CACHE_LEVEL);
        assert_eq!(c(offset_of!(CACHE_RELATIONSHIP, CacheSize)), CACHE_SIZE);
        assert_eq!(c(offset_of!(CACHE_RELATIONSHIP, Type)), CACHE_TYPE);
        assert_eq!(
            c(offset_of!(CACHE_RELATIONSHIP, Anonymous)),
            CACHE_GROUP_MASK
        );
    }

    #[test]
    fn hybrid_e_cores_have_a_lower_efficiency_class() {
        let t = Topology {
            cpus: parse_cpu_sets(&fixture("hybrid_p_e.hex")).unwrap(),
            caches: vec![],
        };
        assert_eq!(
            (t.cpus.len(), t.physical_cores(), t.llc_domains()),
            (12, 8, 1)
        );
        assert!(t.is_hybrid());
        let class = |i: usize| t.cpus[i].efficiency_class;
        assert!(class(8) < class(0));
        assert_eq!((class(0), class(8)), (1, 0));
    }

    #[test]
    fn dual_ccd_reports_two_llc_domains() {
        let cpus = parse_cpu_sets(&fixture("dual_ccd.hex")).unwrap();
        let caches = parse_caches(&fixture("dual_ccd_caches.hex")).unwrap();
        let t = Topology { cpus, caches };
        assert_eq!(
            (t.cpus.len(), t.physical_cores(), t.llc_domains()),
            (16, 8, 2)
        );
        assert!(!t.is_hybrid());
        assert_eq!(t.caches.len(), 2);
        assert_eq!(
            (t.caches[0].level, t.caches[0].kind, t.caches[0].size_bytes),
            (3, CacheKind::Unified, 32 << 20)
        );
        assert_eq!(
            (t.caches[0].group_mask, t.caches[1].group_mask),
            (0x00FF, 0xFF00)
        );
    }

    #[test]
    fn malformed_buffers_are_errors_not_panics() {
        for name in ["truncated.hex", "size_zero.hex"] {
            assert!(
                matches!(
                    parse_cpu_sets(&fixture(name)),
                    Err(TopologyError::BadRecord { .. })
                ),
                "{name}"
            );
        }
        // Every prefix of a valid buffer, and size fields poisoned one byte at a time, must not panic.
        let good = fixture("hybrid_p_e.hex");
        for n in 0..good.len() {
            let _ = parse_cpu_sets(&good[..n]);
        }
        let caches = fixture("dual_ccd_caches.hex");
        for n in 0..caches.len() {
            let _ = parse_caches(&caches[..n]);
        }
        for (i, v) in [(0usize, 0xFFu8), (1, 0xFF), (3, 0xFF), (0, 0), (0, 7)] {
            let mut b = good.clone();
            b[i] = v;
            let _ = parse_cpu_sets(&b);
            let mut b = caches.clone();
            b[4 + i] = v;
            let _ = parse_caches(&b);
        }
        assert!(parse_caches(&caches[..40]).is_err());
        assert!(parse_cpu_sets(&[]).unwrap().is_empty());
    }

    /// Calibrated to the target machine (i5-11400H): 6 identical cores, 12 threads, one LLC, not hybrid.
    #[test]
    fn real_api_matches_the_target_machine() {
        let t = read().unwrap();
        assert_eq!(t.cpus.len(), 12);
        assert_eq!(t.physical_cores(), 6);
        assert_eq!(t.llc_domains(), 1);
        assert!(!t.is_hybrid());
        assert!(
            t.caches.iter().any(|c| c.level == 3),
            "no L3 cache reported: {:?}",
            t.caches
        );
    }

    /// CPUID cross-check: a validation and learning device, NOT a data source (the API above is the source of truth).
    ///
    /// Each logical CPU is visited by pinning this test thread to it (our own thread only), then leaf 0x1F (or 0x0B)
    /// gives the x2APIC id and the SMT shift: `id >> shift` identifies the physical core. Mismatches with the Windows
    /// model are logged; only a different number of logical CPUs fails the test.
    #[test]
    fn cpuid_agrees_with_the_windows_model() {
        use fa_asm::cpuid::cpuid_count;
        let max = cpuid_count(0, 0)[0];
        let leaf = if max >= 0x1F { 0x1F } else { 0x0B };
        if max < leaf || cpuid_count(leaf, 0)[1] & 0xFFFF == 0 {
            eprintln!(
                "topology cross-check skipped: leaf 0x{leaf:X} absent (max basic leaf 0x{max:X})"
            );
            return;
        }
        let model = read().unwrap();
        let mut seen = Vec::new();
        for cpu in &model.cpus {
            let _pin = crate::thread::pin_current_thread(cpu.index).unwrap();
            let mut tries = 0;
            while crate::thread::current_cpu() != cpu.index {
                tries += 1;
                assert!(tries < 1000, "could not move to logical CPU {}", cpu.index);
                std::thread::yield_now();
            }
            let [eax, ebx, _, edx] = cpuid_count(leaf, 0);
            let core_type = if max >= 0x1A {
                cpuid_count(0x1A, 0)[0] >> 24
            } else {
                0
            };
            eprintln!(
                "cpu {:2}: x2apic {edx:#x} smt_shift {} threads/core {} core type {core_type:#x}",
                cpu.index,
                eax & 0x1F,
                ebx & 0xFFFF
            );
            seen.push((edx, eax & 0x1F, ebx & 0xFFFF));
        }
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(
            seen.len(),
            model.cpus.len(),
            "CPUID and Windows disagree on the number of logical CPUs"
        );
        let mut cores: Vec<u32> = seen.iter().map(|&(id, shift, _)| id >> shift).collect();
        cores.dedup();
        if cores.len() != model.physical_cores() {
            eprintln!(
                "MISMATCH physical cores: CPUID {} vs Windows {}",
                cores.len(),
                model.physical_cores()
            );
        }
        let threads = seen[0].2 as usize;
        if threads * model.physical_cores() != model.cpus.len() {
            eprintln!(
                "MISMATCH threads/core: CPUID {threads} vs Windows {}",
                model.cpus.len() / model.physical_cores()
            );
        }
    }
}
