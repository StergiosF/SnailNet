param(
    [string]$KeyPath = (Join-Path $env:USERPROFILE '.tauri\snailnet-updater.key'),
    [string]$Notes = '',
    [string]$TargetDir = ''
)
# Builds the release installer end to end (PLAN.md §17.1, §17.2):
#   1. cargo build --release of snailnet-service (the installer bundles
#      target\release\snailnet-service.exe next to the app),
#   2. the app through the Tauri bundler with tauri.release.conf.json
#      (NSIS, per-machine, service hooks, WinDivert files, updater
#      artifacts signed with the updater key),
#   3. THIRD-PARTY-NOTICES.txt into the target folder (bundled under
#      licenses\ next to WinDivert's licence; scripts\third-party-notices.ps1),
#   4. latest.json for the GitHub release, pointing at
#      https://github.com/StergiosF/SnailNet/releases/download/v<version>/.
# Output: target\release\bundle\nsis\ (SnailNet_<version>_x64-setup.exe,
# its .sig, and latest.json). Upload all three to the release v<version>.
#
# The updater's private key never lives in the repo: by default it is read
# from %USERPROFILE%\.tauri\snailnet-updater.key (no password). Losing it
# means installed copies can't take updates any more: back it up offline.
# -TargetDir builds into another cargo target folder (e.g. when a running
# dev service locks target\).
$ErrorActionPreference = 'Stop'
$root = Resolve-Path (Join-Path $PSScriptRoot '..')

function Invoke-Step([string]$What, [scriptblock]$Run) {
    Write-Host "== $What"
    & $Run
    if ($LASTEXITCODE -ne 0) { throw "$What failed (exit $LASTEXITCODE)" }
}

if (-not (Test-Path $KeyPath)) {
    throw "Updater key not found at $KeyPath. Generate one with: npx tauri signer generate -w `"$KeyPath`""
}
$env:TAURI_SIGNING_PRIVATE_KEY = (Get-Content $KeyPath -Raw).Trim()
if (-not $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD) { $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = '' }

$target = if ($TargetDir) { $TargetDir } else { Join-Path $root 'target' }
$env:CARGO_TARGET_DIR = $target
$conf = Join-Path $root 'app\src-tauri\tauri.release.conf.json'
if ($TargetDir) {
    # The release config names files in ../../target/ (the service, the
    # notices); point them at the other target folder instead.
    $fwd = ("$target" -replace '\\', '/').TrimEnd('/')
    $json = (Get-Content $conf -Raw) -replace '\.\./\.\./target/', "$fwd/"
    $conf = Join-Path $target 'tauri.release.conf.json'
    New-Item -ItemType Directory -Force $target | Out-Null
    Set-Content -Path $conf -Value $json -Encoding utf8
}

Push-Location $root
try {
    Invoke-Step 'cargo build --release -p snailnet-service' { cargo build --release -p snailnet-service }
} finally { Pop-Location }

Push-Location (Join-Path $root 'app')
try {
    if (-not (Test-Path 'node_modules')) { Invoke-Step 'npm ci' { npm ci } }
    # After npm ci: the notices read the bundled npm packages' licences.
    Invoke-Step 'third-party notices' {
        & (Join-Path $PSScriptRoot 'third-party-notices.ps1') -Out (Join-Path $target 'THIRD-PARTY-NOTICES.txt')
    }
    Invoke-Step 'tauri build (NSIS)' { npm run tauri -- build --config $conf }
} finally { Pop-Location }

$version = (Get-Content (Join-Path $root 'app\src-tauri\tauri.conf.json') -Raw | ConvertFrom-Json).version
$nsis = Join-Path $target 'release\bundle\nsis'
$setup = Get-ChildItem $nsis -Filter "*_${version}_x64-setup.exe" | Select-Object -First 1
if (-not $setup) { throw "No installer for $version in $nsis" }
$sig = Get-Content "$($setup.FullName).sig" -Raw
$url = "https://github.com/StergiosF/SnailNet/releases/download/v$version/$($setup.Name)"
$latest = [ordered]@{
    version   = $version
    notes     = $Notes
    pub_date  = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
    platforms = [ordered]@{
        'windows-x86_64' = [ordered]@{ signature = $sig.Trim(); url = $url }
    }
}
$latestPath = Join-Path $nsis 'latest.json'
# UTF-8 without a BOM: the updater parses the file as JSON.
[IO.File]::WriteAllText($latestPath, ($latest | ConvertTo-Json -Depth 4), (New-Object Text.UTF8Encoding $false))

Write-Host ''
Write-Host "Installer:   $($setup.FullName)"
Write-Host "Signature:   $($setup.FullName).sig"
Write-Host "latest.json: $latestPath"
Write-Host "Publish all three on the GitHub release v$version."
