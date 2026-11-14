# Synthetic shaping test (docs/HANDOFF.md, "Running elevated").
# Two copies of curl (a.exe = 40%, b.exe = 60%) download from Cloudflare
# under `snailnet spike` while ping measures latency. Run elevated:
#   Start-Process powershell -Verb RunAs -Wait -ArgumentList '-NoProfile -ExecutionPolicy Bypass -File scripts\phase0-synthetic.ps1'
# Results land in -OutDir: ping-idle.txt, spike.txt, ping-load.txt, done.txt.
param(
    [string]$OutDir = (Join-Path $env:TEMP 'snailnet-synthetic'),
    [double]$Down = 25,
    [double]$Factor = 0.92,
    [int]$Duration = 32,
    [string]$Target = '1.1.1.1'
)
$ErrorActionPreference = 'Stop'

$repo = Split-Path -Parent $PSScriptRoot
$snailnet = Join-Path $repo 'target\release\snailnet.exe'
if (-not (Test-Path $snailnet)) { throw "build first: cargo build --release ($snailnet missing)" }

New-Item -ItemType Directory -Force $OutDir | Out-Null
$bin = Join-Path $OutDir 'bin'
New-Item -ItemType Directory -Force $bin | Out-Null
foreach ($name in 'a.exe', 'b.exe') {
    Copy-Item (Join-Path $env:SystemRoot 'System32\curl.exe') (Join-Path $bin $name) -Force
}
Remove-Item (Join-Path $OutDir '*.txt') -ErrorAction SilentlyContinue

# Idle latency first, before anything is shaped.
ping -n 10 $Target | Out-File -Encoding utf8 (Join-Path $OutDir 'ping-idle.txt')

$spike = Start-Process $snailnet -PassThru -NoNewWindow `
    -ArgumentList @('spike', '--down', $Down, '--factor', $Factor,
        '--app', 'a.exe=40', '--app', 'b.exe=60', '--duration', $Duration) `
    -RedirectStandardOutput (Join-Path $OutDir 'spike.txt') `
    -RedirectStandardError (Join-Path $OutDir 'spike-err.txt')
Start-Sleep -Seconds 2

# One stream per process (parallel streams stall); repeat so both stay busy
# for the whole run. 100 MB requests return 403, so use 25 MB.
$url = 'https://speed.cloudflare.com/__down?bytes=25000000'
$loads = foreach ($name in 'a.exe', 'b.exe') {
    $exe = Join-Path $bin $name
    Start-Process cmd.exe -PassThru -WindowStyle Hidden `
        -ArgumentList "/c for /L %i in (1,1,6) do `"$exe`" -s -o NUL `"$url`""
}
Start-Sleep -Seconds 4

# Latency under load.
ping -n ([Math]::Max(5, $Duration - 10)) $Target | Out-File -Encoding utf8 (Join-Path $OutDir 'ping-load.txt')

$spike.WaitForExit(($Duration + 15) * 1000) | Out-Null
foreach ($p in $loads) { Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue }
Get-Process a, b -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
"spike exit code: $($spike.ExitCode)" | Out-File -Encoding utf8 (Join-Path $OutDir 'done.txt')
