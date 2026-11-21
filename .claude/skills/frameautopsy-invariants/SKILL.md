---
name: frameautopsy-invariants
description: FrameAutopsy project invariants. Use before writing or reviewing any Rust in this repo, especially anything touching unsafe, asm, SIMD, FFI, hot paths, benches or scripts/gate.ps1. Covers unsafe confinement, the earn-its-place rule, zero allocation on the hot path and the gate.
---

# FrameAutopsy invariants

Full rules: `CLAUDE.md`, `docs/asm-unsafe-catalog.md`, spec `unsafe-policy`. This is the checklist.

## Unsafe confinement
- `unsafe` only in `fa-asm`, `fa-win`, `fa-sync`. Every other crate starts with `#![forbid(unsafe_code)]`
  (the gate's `headers` step fails otherwise).
- Unsafe crates keep three denies: `unsafe_op_in_unsafe_fn`, `clippy::undocumented_unsafe_blocks`,
  `clippy::multiple_unsafe_ops_per_block`. One unsafe operation per block, uppercase `// SAFETY:` stating the invariant,
  also on `unsafe impl`. Public API stays safe.
- Every `extern "system"` callback wraps its body in `catch_unwind`.

## Earn its place (asm, SIMD, unsafe sync)
1. Write the catalog line in `docs/asm-unsafe-catalog.md` BEFORE the code.
2. Reference implementation, parity test, recorded comparison on the declared metric, runtime fallback.
3. Loses, or wins by less than 5 %: the reference ships, the module is deleted, the numbers stay in the catalog.
4. Asm is `core::arch::asm!`, Intel syntax, precise `options`. No MASM/NASM/`build.rs`. `FA_FORCE_SCALAR=1` forces fallbacks.
5. Each new unsafe/asm block gets a French explanation (catalog section 7); merge only after the user says "compris".

## Zero allocation on the hot path
- ETW callback -> SPSC ring -> window -> trigger never allocates. Prove it with `fa_sync::counting`
  (`assert_no_alloc`, feature `test-support`), not by reading the code. Pre-sized, reused `Vec`s.

## Gate
- `powershell -File scripts/gate.ps1`: headers, fmt, clippy `--all-features -D warnings`, test, (Miri from lot 5), graphify.
  Green before moving to the next lot. A new guard test must be shown to fail first (skill `mutation-verified-testing`).
