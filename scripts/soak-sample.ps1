# Soak sampler (PLAN.md §21 Phase 1 exit: flat memory, no handle leaks).
# Appends one CSV row per run with the SnailNet service's memory and handle
# counts. No admin needed. Run it every 10 min from a scheduled task, e.g.:
#   conhost.exe --headless powershell -NoProfile -ExecutionPolicy Bypass -File scripts\soak-sample.ps1
# Columns: time (ISO 8601, local), pid, ws_mb, private_mb, handles, threads.
# A missing service writes a row with pid 0, so restarts and gaps show up.
param([string]$Out = (Join-Path $env:LOCALAPPDATA 'SnailNetSoak\soak.csv'))

$inv = [Globalization.CultureInfo]::InvariantCulture
New-Item -ItemType Directory -Force (Split-Path $Out) | Out-Null
if (-not (Test-Path $Out)) {
    'time,pid,ws_mb,private_mb,handles,threads' | Out-File -Encoding utf8 $Out
}
$time = Get-Date -Format 's'
$p = Get-Process snailnet-service -ErrorAction SilentlyContinue | Select-Object -First 1
$row = if ($p) {
    $ws = ([math]::Round($p.WorkingSet64 / 1MB, 1)).ToString($inv)
    $priv = ([math]::Round($p.PrivateMemorySize64 / 1MB, 1)).ToString($inv)
    "$time,$($p.Id),$ws,$priv,$($p.HandleCount),$($p.Threads.Count)"
} else {
    "$time,0,,,,"
}
$row | Out-File -Encoding utf8 -Append $Out
