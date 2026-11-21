# Local gate: stops at the first failing step (exit code != 0).
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot -Parent)

function Step($name, [scriptblock]$cmd) {
    Write-Host "== $name"
    & $cmd
    if ($LASTEXITCODE -ne 0) { Write-Host "GATE FAILED at: $name"; exit $LASTEXITCODE }
}

Step 'fmt'    { cargo fmt --all -- --check }
Step 'clippy' { cargo clippy --workspace --all-targets -- -D warnings }
Step 'test'   { cargo test --workspace --all-features }
# Lot 5+: Miri on fa-sync once it has tests. Lot 10+: `fa lab score`.
if (Test-Path crates/fa-sync/tests) { Step 'miri' { cargo +nightly miri test -p fa-sync } }
if (Test-Path crates/fa) { Step 'lab score' { cargo run -p fa --release -- lab score } }
Step 'graphify' { graphify update . }
Write-Host 'GATE OK'
