# Dev check of the Lab against the lot 5 capture (tasks 4.2): run from an ELEVATED terminal (ETW needs admin).
#   powershell -File scripts\lab-verify.ps1 <run-name> [scenario.toml]
# Writes ours-<run-name>.csv (capture) and ours-<run-name>.jsonl (journal) at the repository root, both gitignored,
# then compares them with `fa lab verify`. Use a new run name each time: an existing name is overwritten.
param(
    [Parameter(Mandatory = $true)][string]$Run,
    [string]$Scenario = 'lab\scenarios\frame-stall.toml'
)
$ErrorActionPreference = 'Stop'
$admin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $admin) { Write-Error 'Terminal non eleve : relancez dans un PowerShell ouvert en administrateur.'; exit 2 }
Set-Location (Split-Path $PSScriptRoot)
$root = (Get-Location).Path -replace '\\', '/'
$csv = "$root/ours-$Run.csv"
$journal = "$root/ours-$Run.jsonl"
$fa = 'target\release\fa.exe'
if (-not (Test-Path $fa)) { Write-Error 'Compilez d abord : cargo build -p fa --release'; exit 2 }

# The capture starts first and outlives the scenario (3 s baseline + scenario + margin).
$cap = Start-Process $fa -ArgumentList 'capture', '--seconds', '45', '--frames-csv', $csv -NoNewWindow -PassThru
Start-Sleep -Seconds 3
& $fa lab run $Scenario --journal $journal
if ($LASTEXITCODE -ne 0) { $cap.Kill(); exit $LASTEXITCODE }
$cap.WaitForExit()
& $fa lab verify $journal $csv
exit $LASTEXITCODE
