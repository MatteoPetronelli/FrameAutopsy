# Third-party Claude Code skills

Community skills vendored into `.claude/skills/`. Each was read in full before being copied (no network calls on
activation, no access to secrets, no hooks or scripts). Downloaded 2026-11-20. To update, re-download from the source at
a new commit, re-audit, and update this file. Do not edit the vendored files in place.

## mohitmishra786/low-level-dev-skills

- Source: https://github.com/mohitmishra786/low-level-dev-skills
- Commit: `18f29518a47fda070e6ec1268cb577692fae9a10`
- License: MIT, see `LICENSE-low-level-dev-skills`
- Copied unchanged (`SKILL.md` + `references/`), flattened from `skills/<category>/<name>/` to `.claude/skills/<name>/`:
  `rust-unsafe`, `rust-sanitizers-miri`, `rust-ffi`, `rust-profiling` (from `skills/rust/`);
  `memory-model`, `cpu-cache-opt`, `simd-intrinsics`, `assembly-x86` (from `skills/low-level-programming/`).
- "Related skills" links point to skills of the source repo that are not vendored here.

Audit notes:
- The examples target Linux, GCC/Clang and the System V ABI. FrameAutopsy targets Windows x64 (Win64 ABI), Rust `asm!`
  with Intel syntax and `windows-rs`. `perf`, `sudo`, `valgrind`, futex and rseq content does not apply.
- `rust-ffi`: the `unsafe impl Sync for MyLib {}` example has a wrong justification ("not shared across threads").
  Do not copy it.
- `rust-sanitizers-miri`: `-Zmiri-tag-raw-pointers` and `-Zmiri-check-number-validity` are obsolete Miri flags; Miri has
  its own data-race detector (not "via Stacked Borrows").
- `rust-unsafe`: examples write `// Safety:`; this repo requires `// SAFETY:` (Clippy `undocumented_unsafe_blocks`).

## trailofbits/skills

- Source: https://github.com/trailofbits/skills
- Commit: `149d7b21f4067765b82e67029730b42d228ef4b4`
- License: Creative Commons Attribution-ShareAlike 4.0 International (CC BY-SA 4.0), see `LICENSE-trailofbits-skills`.
  © Trail of Bits. These files stay under CC BY-SA 4.0 in this repository.
- `property-based-testing` (from `plugins/property-based-testing/skills/property-based-testing/`): `SKILL.md` and
  `references/` copied unchanged.
- `sharp-edges` (from `plugins/sharp-edges/skills/sharp-edges/`): `SKILL.md` unchanged; `references/` limited to
  `auth-patterns`, `case-studies`, `config-patterns`, `crypto-apis`, `lang-c`, `lang-rust`, `language-specific`.
- Modifications: non-Claude files removed (`agents/openai.yaml`, `assets/*.svg`, `README.md`); in `sharp-edges`, the
  language guides for C#, Go, Java, JavaScript, Kotlin, PHP, Python, Ruby and Swift were omitted (not used in this
  Rust/C codebase; `language-specific.md` keeps a short summary of each). Links to them in `SKILL.md` are dead on purpose.
- The `sharp-edges-analyzer` agent mentioned in `sharp-edges/SKILL.md` is part of the Trail of Bits plugin and is not
  installed here: run the four-phase workflow inline.
