//! Kernel module map: base, size and file name of every loaded kernel image, so a kernel address resolves to
//! module + offset. Lives on the worker (it may allocate), fed with the cold image records.
//!
//! The spike (2026-11-22, build 26300) showed the addresses are real, so resolution is on. If a future Windows masks
//! them, build the map with `addresses_available = false`: it then keeps names only and resolves no address.

use crate::record::{ColdRecord, ImageKind};

/// First address of kernel space on x64 Windows.
pub const KERNEL_FLOOR: u64 = 0xFFFF_8000_0000_0000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Module {
    pub base: u64,
    pub size: u64,
    pub name: String,
}

#[derive(Debug)]
pub struct ModuleMap {
    addresses_available: bool,
    /// Sorted by `base`.
    modules: Vec<Module>,
}

impl ModuleMap {
    pub fn new(addresses_available: bool) -> Self {
        Self {
            addresses_available,
            modules: Vec::new(),
        }
    }

    /// Feeds one record: rundown and live loads add a module, an unload removes it. User-mode images are ignored.
    pub fn apply(&mut self, record: &ColdRecord) {
        let ColdRecord::Image {
            base,
            size,
            kind,
            name,
            ..
        } = record
        else {
            return;
        };
        if *base < KERNEL_FLOOR {
            return;
        }
        let at = self.modules.partition_point(|m| m.base < *base);
        let present = self.modules.get(at).is_some_and(|m| m.base == *base);
        match kind {
            ImageKind::Unload => {
                if present {
                    self.modules.remove(at);
                }
            }
            _ => {
                let module = Module {
                    base: *base,
                    size: *size,
                    name: name.as_str().to_owned(),
                };
                if present {
                    self.modules[at] = module;
                } else {
                    self.modules.insert(at, module);
                }
            }
        }
    }

    pub fn len(&self) -> usize {
        self.modules.len()
    }

    pub fn is_empty(&self) -> bool {
        self.modules.is_empty()
    }

    /// Module name and offset of `address`, or `None` when it is in no known module (or addresses are unavailable).
    pub fn resolve(&self, address: u64) -> Option<(&str, u64)> {
        if !self.addresses_available {
            return None;
        }
        let at = self
            .modules
            .partition_point(|m| m.base <= address)
            .checked_sub(1)?;
        let m = &self.modules[at];
        (address - m.base < m.size).then(|| (m.name.as_str(), address - m.base))
    }

    /// Module names, whatever the address availability (the fallback keeps these).
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.modules.iter().map(|m| m.name.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::Name;

    fn image(base: u64, size: u64, kind: ImageKind, name: &str) -> ColdRecord {
        ColdRecord::Image {
            ts: 0,
            base,
            size,
            pid: 0,
            kind,
            name: Name::from_utf16_basename(name.encode_utf16()),
        }
    }

    fn map(available: bool) -> ModuleMap {
        let mut m = ModuleMap::new(available);
        m.apply(&image(
            0xffff_f804_9f40_0000,
            0x0145_0000,
            ImageKind::DcStart,
            "ntoskrnl.exe",
        ));
        m.apply(&image(
            0xffff_f804_a0c0_0000,
            0x2000,
            ImageKind::DcStart,
            "tiny.sys",
        ));
        m.apply(&image(
            0xffff_f804_2000_0000,
            0x1000,
            ImageKind::Load,
            "hal.dll",
        ));
        m
    }

    #[test]
    fn an_address_inside_a_module_resolves_to_module_and_offset() {
        let m = map(true);
        assert_eq!(m.resolve(0xffff_f804_9f40_0000), Some(("ntoskrnl.exe", 0)));
        assert_eq!(
            m.resolve(0xffff_f804_9f40_0000 + 0x1234),
            Some(("ntoskrnl.exe", 0x1234))
        );
        assert_eq!(m.resolve(0xffff_f804_a0c0_1fff), Some(("tiny.sys", 0x1fff)));
        assert_eq!(m.len(), 3);
    }

    #[test]
    fn an_address_outside_every_module_does_not_resolve() {
        let m = map(true);
        assert_eq!(
            m.resolve(0xffff_f804_a0c0_2000),
            None,
            "one past the end of tiny.sys"
        );
        assert_eq!(
            m.resolve(0xffff_f804_1fff_ffff),
            None,
            "before the first module"
        );
        assert_eq!(m.resolve(0), None);
        assert_eq!(m.resolve(u64::MAX), None);
    }

    #[test]
    fn an_unload_removes_the_module_and_user_images_are_ignored() {
        let mut m = map(true);
        m.apply(&image(
            0xffff_f804_a0c0_0000,
            0x2000,
            ImageKind::Unload,
            "tiny.sys",
        ));
        assert_eq!(m.resolve(0xffff_f804_a0c0_0010), None);
        m.apply(&image(
            0x7ff6_0000_0000,
            0x1000,
            ImageKind::Load,
            "chrome.exe",
        ));
        assert_eq!(m.len(), 2);
    }

    #[test]
    fn when_addresses_are_unavailable_only_names_are_kept() {
        let m = map(false);
        assert_eq!(m.resolve(0xffff_f804_9f40_0010), None);
        let mut names: Vec<_> = m.names().collect();
        names.sort_unstable();
        assert_eq!(names, ["hal.dll", "ntoskrnl.exe", "tiny.sys"]);
    }
}
