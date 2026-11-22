# lockfree-transport Specification

## Purpose
The single-producer single-consumer ring that carries decoded events from the ETW callback to the worker: it never
blocks, counts what it drops, and is verified under Miri against the `rtrb` reference.

## Requirements

### Requirement: SPSC ring
`fa-sync` SHALL provide a single-producer single-consumer ring: power-of-two capacity, `T: Copy`, cache-line-padded
head and tail, Acquire/Release ordering, `MaybeUninit<T>` slots. `push` SHALL never block: when full it returns an error
and increments a drop counter.

#### Scenario: Consumer stalls
- **WHEN** the consumer stops reading and the ring fills
- **THEN** the ETW callback keeps returning immediately and the drop counter grows

### Requirement: Verified under Miri
A two-thread stress test SHALL pass under Miri.

#### Scenario: Two-thread stress
- **WHEN** `cargo +nightly miri test -p fa-sync` runs the two-thread stress test
- **THEN** it passes with no undefined behaviour reported

### Requirement: Earns its place against rtrb
A criterion bench (32-byte records) SHALL show throughput at least equal to `rtrb`; otherwise `rtrb` replaces the
module. Prefetch or non-temporal variants ship only if they win.

#### Scenario: Bench against rtrb
- **WHEN** the 32-byte-record bench runs against `rtrb` (3 runs, median and spread)
- **THEN** the module ships only with throughput at least equal and a win of 5 % or more, otherwise `rtrb` replaces it and the module is deleted
