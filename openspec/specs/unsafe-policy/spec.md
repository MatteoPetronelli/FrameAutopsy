# unsafe-policy Specification

## Purpose
Make `unsafe` and assembly earn their place, confine them to three crates, and keep every block explainable.

## Requirements

### Requirement: Unsafe confinement
Only `fa-asm`, `fa-win` and `fa-sync` MAY contain `unsafe`. Every other crate SHALL declare `#![forbid(unsafe_code)]`.

#### Scenario: Unsafe in a safe crate
- **WHEN** an `unsafe` block is added to `fa-core`
- **THEN** compilation fails

### Requirement: Lints in unsafe crates
The three unsafe crates SHALL declare `#![deny(unsafe_op_in_unsafe_fn)]` and deny the Clippy lints
`undocumented_unsafe_blocks` and `multiple_unsafe_ops_per_block`. Every `unsafe` block SHALL carry a `// SAFETY:`
comment that states the invariant making it sound.

#### Scenario: Missing SAFETY comment
- **WHEN** an `unsafe` block without `// SAFETY:` is added to `fa-win`
- **THEN** the gate fails on Clippy

### Requirement: Safe public API
The public API of the three unsafe crates SHALL be safe. An exported `unsafe fn` is allowed only with a `# Safety`
doc section and a written reason in `docs/asm-unsafe-catalog.md`.

#### Scenario: Exported unsafe fn
- **WHEN** an `unsafe fn` is exported by `fa-asm`, `fa-win` or `fa-sync`
- **THEN** it has a `# Safety` doc section and a written reason in `docs/asm-unsafe-catalog.md`

### Requirement: Earn-its-place rule
Every asm or unsafe module SHALL have (a) a reference implementation, (b) a parity test against it, (c) a recorded
comparison on its declared metric (criterion bench or comparative test) in `docs/asm-unsafe-catalog.md`, and (d) a
runtime fallback. If the module does not beat the reference on its declared metric, the reference SHALL ship and the
module SHALL be removed. FFI with no safe alternative is exempt from (c), but an existing wrapper crate that meets the
requirements counts as the reference.

#### Scenario: Kernel loses its benchmark
- **WHEN** the asm Stream VByte decoder is slower than the intrinsics version on the user's machine
- **THEN** the intrinsics version ships, the asm version is deleted, and the catalog records both numbers

#### Scenario: Probe brings no extra information
- **WHEN** the frequency probe shows nothing that the PDH counter does not already show (see `driverless-probes`)
- **THEN** the probe is removed and the PDH counter is used

### Requirement: Runtime dispatch
CPU features SHALL be detected once at startup (`is_x86_feature_detected!`, plus CPUID for WAITPKG and MWAITX) and
the implementation selected through a function pointer or enum. Setting `FA_FORCE_SCALAR=1` SHALL force every
fallback path.

#### Scenario: CPU without AVX2
- **WHEN** the test suite runs with `FA_FORCE_SCALAR=1`
- **THEN** all tests pass on the scalar paths and no AVX2 instruction executes

### Requirement: Assembly form
Assembly SHALL use `core::arch::asm!` with Intel syntax and precise options (`nomem`, `nostack`, `preserves_flags`
only when true). `global_asm!` SHALL be used only when a whole function is required, and then with
`.seh_proc`/`.seh_endproc` unwind directives.

#### Scenario: Asm syntax
- **WHEN** assembly is added to an unsafe crate
- **THEN** it uses `core::arch::asm!` with Intel syntax, or `global_asm!` with `.seh_proc`/`.seh_endproc` only when a whole function is required

### Requirement: No unwinding across FFI
Every `extern "system"` callback SHALL wrap its body in `std::panic::catch_unwind`; a caught panic stops the related
session and is recorded.

#### Scenario: Callback panic
- **WHEN** an `extern "system"` callback panics
- **THEN** `std::panic::catch_unwind` catches it, the related session stops and the panic is recorded

### Requirement: Miri coverage
`fa-sync` tests SHALL pass under `cargo +nightly miri test -p fa-sync`, with iteration counts reduced under
`cfg(miri)`.

#### Scenario: Miri run
- **WHEN** `cargo +nightly miri test -p fa-sync` is run
- **THEN** the `fa-sync` tests pass, with iteration counts reduced under `cfg(miri)`

### Requirement: Explanation checkpoint
Each change that adds or modifies an `unsafe` or asm block SHALL include, in its `tasks.md`, a checkpoint where the
block is explained to the user in French (what it does, why it is sound, what would break it). The block is merged only
after the user confirms understanding.

#### Scenario: Checkpoint in tasks
- **WHEN** a change adds or modifies an `unsafe` or asm block
- **THEN** its `tasks.md` has a checkpoint explaining the block to the user in French, and the block is merged only after the user confirms understanding
