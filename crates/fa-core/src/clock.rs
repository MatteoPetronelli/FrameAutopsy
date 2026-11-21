//! Pure clock logic (no hardware access): timebase choice now, tick conversion and the QPC<->TSC map in later tasks.

/// The counter every timestamp is expressed in. Raw TSC ticks everywhere unless the CPU has no invariant TSC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Timebase {
    Tsc,
    /// Degraded mode: ticks are QPC counts. The health check reports it.
    Qpc,
}

impl Timebase {
    pub fn from_invariant(invariant_tsc: bool) -> Self {
        if invariant_tsc {
            Timebase::Tsc
        } else {
            Timebase::Qpc
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_invariant_tsc_falls_back_to_qpc() {
        assert_eq!(Timebase::from_invariant(true), Timebase::Tsc);
        assert_eq!(Timebase::from_invariant(false), Timebase::Qpc);
    }
}
