# Dev check of the Lab against the lot 5 capture (tasks 4.2 and 4.3): run from an ELEVATED terminal (ETW needs admin).
#   powershell -File scripts\lab-verify.ps1 <run-name> [scenario.toml] [-PresentMon]
# Writes measurements\ours-<run-name>.csv (capture) and measurements\ours-<run-name>.jsonl (journal), gitignored,
# then compares them with `fa lab verify`. Use a new run name each time: an existing name is overwritten.
# With -PresentMon, PresentMon 2.6.0 records fa.exe at the same time (measurements\pm-<run-name>.csv) and the
# frametime parity of the lot 5 (`fa capture compare`, 0.1 ms) is replayed on the Lab loop.
param(
    [Parameter(Mandatory = $true)][string]$Run,
    [string]$Scenario = 'lab\scenarios\frame-stall.toml',
    [switch]$PresentMon
)
$ErrorActionPreference = 'Stop'
$admin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $admin) { Write-Error 'Terminal non eleve : relancez dans un PowerShell ouvert en administrateur.'; exit 2 }
Set-Location (Split-Path $PSScriptRoot)
$root = (Get-Location).Path -replace '\\', '/'
New-Item -ItemType Directory -Force "$root/measurements/lab-runs" | Out-Null
$csv = "$root/measurements/lab-runs/ours-$Run.csv"
$journal = "$root/measurements/lab-runs/ours-$Run.jsonl"
$pmCsv = "$root/measurements/lab-runs/pm-$Run.csv"
$pmExe = 'C:\Program Files\Intel\PresentMon\PresentMonConsoleApplication\PresentMon-2.6.0-x64.exe'
$fa = 'target\release\fa.exe'
if (-not (Test-Path $fa)) { Write-Error 'Compilez d abord : cargo build -p fa --release'; exit 2 }

if ($PresentMon) {
    if (-not (Test-Path $pmExe)) { Write-Error 'PresentMon 2.6.0 introuvable'; exit 2 }
    $pm = Start-Process $pmExe -ArgumentList '--process_name', 'fa.exe', '--output_file', $pmCsv, '--timed', '32', '--terminate_after_timed', '--stop_existing_session' -NoNewWindow -PassThru
}
# The capture starts first and outlives the scenario (3 s baseline + scenario + margin).
$cap = Start-Process $fa -ArgumentList 'capture', '--seconds', '45', '--frames-csv', $csv -NoNewWindow -PassThru
Start-Sleep -Seconds 3
& $fa lab run $Scenario --journal $journal
if ($LASTEXITCODE -ne 0) { $cap.Kill(); exit $LASTEXITCODE }
$cap.WaitForExit()
& $fa lab verify $journal $csv
$code = $LASTEXITCODE
& $fa lab effect $journal $csv
if ($PresentMon) {
    $pm.WaitForExit()
    & $fa capture compare $csv $pmCsv
}
exit $code
