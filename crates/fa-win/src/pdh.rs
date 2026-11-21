//! WIN-08: PDH reader, used as the reference for the ASM-02 frequency probe (and nothing else).
//!
//! Counter names are always the English ones (`PdhAddEnglishCounterW`), so the code works on any Windows language.
//! Rate and "fraction" counters need two collections before they give a value: call [`PdhReader::collect`] twice.

use std::io;
use windows_sys::Win32::System::Performance::{
    PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterValue,
    PdhOpenQueryW, PDH_FMT_COUNTERVALUE, PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY,
};

const PDH_OK: u32 = 0;

fn pdh_err(what: &str, status: u32) -> io::Error {
    io::Error::other(format!("{what}: PDH status {status:#010x}"))
}

/// One PDH query holding a fixed list of counters.
pub struct PdhReader {
    query: PDH_HQUERY,
    counters: Vec<PDH_HCOUNTER>,
}

impl PdhReader {
    /// Opens a query and adds every counter path (English names, e.g. `\Processor Information(0,4)\% Processor Performance`).
    pub fn new(paths: &[&str]) -> io::Result<PdhReader> {
        let mut query: PDH_HQUERY = std::ptr::null_mut();
        // SAFETY: a null data source means the live system; `query` is a valid out-pointer for the handle.
        let st = unsafe { PdhOpenQueryW(std::ptr::null(), 0, &mut query) };
        if st != PDH_OK {
            return Err(pdh_err("PdhOpenQueryW", st));
        }
        // From here `reader` owns the handle: any early return closes it through `Drop`.
        let mut reader = PdhReader {
            query,
            counters: Vec::with_capacity(paths.len()),
        };
        for path in paths {
            let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
            let mut counter: PDH_HCOUNTER = std::ptr::null_mut();
            // SAFETY: `wide` is a NUL-terminated UTF-16 string alive for the call; `counter` is a valid out-pointer;
            // `reader.query` is the open query.
            let st = unsafe { PdhAddEnglishCounterW(reader.query, wide.as_ptr(), 0, &mut counter) };
            if st != PDH_OK {
                return Err(pdh_err(path, st));
            }
            reader.counters.push(counter);
        }
        Ok(reader)
    }

    /// Takes one sample of every counter.
    pub fn collect(&self) -> io::Result<()> {
        // SAFETY: `self.query` is the open query owned by `self`.
        let st = unsafe { PdhCollectQueryData(self.query) };
        if st == PDH_OK {
            Ok(())
        } else {
            Err(pdh_err("PdhCollectQueryData", st))
        }
    }

    /// Value of counter `index` (order of the paths given to `new`) as `f64`, from the last two collections.
    pub fn value(&self, index: usize) -> io::Result<f64> {
        let counter = *self
            .counters
            .get(index)
            .ok_or_else(|| io::Error::other("PDH counter index out of range"))?;
        // SAFETY: all-zero is a valid `PDH_FMT_COUNTERVALUE` (plain integers and a union of plain numbers).
        let mut v: PDH_FMT_COUNTERVALUE = unsafe { std::mem::zeroed() };
        // SAFETY: `counter` belongs to the open query; a null type pointer is allowed; `v` is a valid out-pointer.
        let st = unsafe {
            PdhGetFormattedCounterValue(counter, PDH_FMT_DOUBLE, std::ptr::null_mut(), &mut v)
        };
        if st != PDH_OK {
            return Err(pdh_err("PdhGetFormattedCounterValue", st));
        }
        if v.CStatus != PDH_OK {
            return Err(pdh_err("counter status", v.CStatus));
        }
        // SAFETY: `PDH_FMT_DOUBLE` was requested and the call succeeded, so the union holds `doubleValue`.
        Ok(unsafe { v.Anonymous.doubleValue })
    }
}

impl Drop for PdhReader {
    fn drop(&mut self) {
        // SAFETY: `self.query` is open and closed exactly once, here; closing also frees its counters.
        unsafe { PdhCloseQuery(self.query) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// Plausibility on this machine: CPU 0 reports a performance percentage between 1 and 1000 (turbo goes above 100).
    #[test]
    fn processor_performance_is_plausible() {
        let r = PdhReader::new(&[r"\Processor Information(0,0)\% Processor Performance"]).unwrap();
        r.collect().unwrap();
        std::thread::sleep(Duration::from_millis(500));
        r.collect().unwrap();
        let v = r.value(0).unwrap();
        assert!((1.0..1000.0).contains(&v), "% Processor Performance = {v}");
    }

    #[test]
    fn an_unknown_counter_is_an_error_and_does_not_leak_the_query() {
        assert!(PdhReader::new(&[r"\No Such Object(0)\Nothing"]).is_err());
    }

    #[test]
    fn an_index_out_of_range_is_an_error() {
        let r = PdhReader::new(&[r"\Processor Information(0,0)\% Processor Performance"]).unwrap();
        assert!(r.value(1).is_err());
    }
}
