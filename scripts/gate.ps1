# Local gate: stops at the first failing step (exit code != 0).
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot -Parent)

function Step($name, [scriptblock]$cmd) {
    Write-Host "== $name"
    & $cmd
    if ($LASTEXITCODE -ne 0) { Write-Host "GATE FAILED at: $name"; exit $LASTEXITCODE }
}

# Lint headers are the unsafe-policy: a crate that loses them would silently allow unsafe.
Step 'headers' {
    $fail = 0
    $need = @{
        'fa-core' = @('#![forbid(unsafe_code)]')
        'fa-asm'  = @('#![deny(unsafe_op_in_unsafe_fn)]', '#![deny(clippy::undocumented_unsafe_blocks)]', '#![deny(clippy::multiple_unsafe_ops_per_block)]')
    }
    $need['fa-win'] = $need['fa-asm']; $need['fa-sync'] = $need['fa-asm']
    foreach ($c in $need.Keys) {
        $src = Get-Content "crates/$c/src/lib.rs" -Raw
        foreach ($h in $need[$c]) { if (-not $src.Contains($h)) { Write-Host "missing in ${c}: $h"; $fail = 1 } }
    }
    # Other crates (lot 1+) must forbid unsafe too.
    Get-ChildItem crates -Directory | Where-Object { $need -notcontains $_.Name -and -not $need.ContainsKey($_.Name) } | ForEach-Object {
        $f = Join-Path $_.FullName 'src/lib.rs'; if (-not (Test-Path $f)) { $f = Join-Path $_.FullName 'src/main.rs' }
        if (-not (Get-Content $f -Raw).Contains('#![forbid(unsafe_code)]')) { Write-Host "missing in $($_.Name): #![forbid(unsafe_code)]"; $fail = 1 }
    }
    $global:LASTEXITCODE = $fail
}
Step 'fmt'    { cargo fmt --all -- --check }
Step 'clippy' { cargo clippy --workspace --all-targets --all-features -- -D warnings }
Step 'test'   { cargo test --workspace --all-features }
# `fa lab score` runs once the Lab scorer (crates/fa/src/lab.rs) exists, lot 10+.
# Lot 5+: Miri once fa-sync has its SPSC ring (src/spsc.rs, SYNC-01); the counting allocator alone is not Miri material. Lot 10+: `fa lab score`.
if (Test-Path crates/fa-sync/src/spsc.rs) { Step 'miri' { cargo +nightly miri test -p fa-sync } }
if (Test-Path crates/fa/src/lab.rs) { Step 'lab score' { cargo run -p fa --release -- lab score } }
Step 'graphify' { graphify update . }
Write-Host 'GATE OK'
