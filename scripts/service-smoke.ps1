param([string]$OutDir = $PSScriptRoot)
# Elevated smoke test of the real service: install, start on defaults
# (monitor only), start with down_mbps = 25 (shaping), stop, dump the logs,
# then uninstall --remove-data. Report: <OutDir>\svc-smoke.txt. Needs a
# release build and Administrator.
$ErrorActionPreference = 'Continue'
$out = Join-Path $OutDir 'svc-smoke.txt'
$exe = Join-Path $PSScriptRoot '..\target\release\snailnet-service.exe'
elease\snailnet-service.exe'
eleasesnailnet-service.exe'
$data = 'C:\ProgramData\SnailNet'
function Log($m) { Add-Content -Path $out -Value $m -Encoding utf8 }
function Q { Log ((sc.exe query SnailNet | Select-String 'STATE') -join ' ') }
function WaitState($s) { for ($i = 0; $i -lt 60; $i++) { if ((sc.exe query SnailNet) -match $s) { return }; Start-Sleep -Milliseconds 250 } }
Set-Content -Path $out -Value "== smoke $(Get-Date -Format o)" -Encoding utf8

Log '== install'; & $exe install *>&1 | ForEach-Object { Log "$_" }
Log ((sc.exe qc SnailNet) -join "`n")
Log ((sc.exe qfailure SnailNet) -join "`n")

Log '== start (defaults: no line speed -> monitor only)'
sc.exe start SnailNet | Out-Null; WaitState 'RUNNING'; Start-Sleep 2; Q
Log '-- config.toml'; Log (Get-Content "$data\config.toml" -Raw)
Log '-- ACL'; Log ((icacls $data) -join "`n")
sc.exe stop SnailNet | Out-Null; WaitState 'STOPPED'; Q

Log '== start with down_mbps = 25 (shaping)'
$cfg = Get-Content "$data\config.toml" -Raw
$cfg = $cfg -replace '(?m)^\[line\]\s*$', "[line]`ndown_mbps = 25.0"
if ($cfg -notmatch 'down_mbps') { $cfg += "`n[line]`ndown_mbps = 25.0`n" }
[IO.File]::WriteAllText("$data\config.toml", $cfg)
sc.exe start SnailNet | Out-Null; WaitState 'RUNNING'; Start-Sleep 8; Q
Log ('-- WinDivert driver: ' + ((sc.exe query WinDivert | Select-String 'STATE') -join ' '))
sc.exe stop SnailNet | Out-Null; WaitState 'STOPPED'; Q
Log ('-- running marker left: ' + (Test-Path "$data\running"))
Log '-- data files'; Log ((Get-ChildItem -Recurse $data | ForEach-Object FullName) -join "`n")
Log '-- logs'; Get-ChildItem "$data\logs" -File | ForEach-Object { Log (Get-Content $_.FullName -Raw) }

Log '== uninstall --remove-data'; & $exe uninstall --remove-data *>&1 | ForEach-Object { Log "$_" }
Log ('-- service gone: ' + ((sc.exe query SnailNet) -join ' '))
Log ('-- data gone: ' + (-not (Test-Path $data)))
Log '== done'
