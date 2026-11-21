# FrameAutopsy

Windows tool for PC gamers that explains and fixes in-game stutters, then proves the fix with paired A/B measurements.
Rust workspace: an elevated headless collector and a non-elevated egui UI. It never touches the game process
(see `docs/compliance.md`). Conventions and rules: [`CLAUDE.md`](CLAUDE.md).

## Build and check
```powershell
cargo build --workspace
powershell -File scripts/gate.ps1   # headers, fmt, clippy -D warnings, test, graphify
```
Stop any running `fa-ui` / `fa-collector` before building: Windows locks running executables.

## Disk hygiene
Dependencies are built without debug info (see `Cargo.toml`). Clean old build artifacts regularly:
```powershell
cargo sweep --time 14
```

## Workflow
One OpenSpec change per lot (`openspec list`, `openspec validate --all`); order and scope in `roadmap.md`.
