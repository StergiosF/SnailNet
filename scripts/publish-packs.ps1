param(
    [string]$KeyPath = (Join-Path $env:USERPROFILE '.tauri\snailnet-updater.key'),
    [switch]$Upload
)
# Publishes the site, app-family and game-folder packs (PLAN.md §17.2)
# so installed copies pick them up without a new build:
#   1. packs.toml from assets\sites.toml, assets\families.toml and
#      assets\games.toml, with format = 1 and serial = the UTC time now
#      (yyyyMMddHHmm; the service keeps a bundle only if its serial is
#      above the one it runs),
#   2. packs.toml.minisig: signed with the updater key through
#      `tauri signer sign`, decoded from its base64 wrapper to the plain
#      minisign text the service checks,
#   3. assets\packs-serial.txt set to the same serial, so builds from now
#      on count these lists as their own (commit it with the assets),
#   4. with -Upload: both files onto the GitHub release `packs` (made on
#      first use as a pre-release, never "latest", so the app updater's
#      releases/latest/download/latest.json is not disturbed).
# Output: target\packs\. The service fetches
# https://github.com/StergiosF/SnailNet/releases/download/packs/packs.toml
# (and .minisig) once a day; the repo must be public for that to work.
$ErrorActionPreference = 'Stop'
$root = Resolve-Path (Join-Path $PSScriptRoot '..')

# Native tools write progress to stderr; under 'Stop' PowerShell 5.1 would
# turn that into an error when output is redirected. Judge by exit code.
function Invoke-Native([string]$What, [scriptblock]$Run) {
    $ErrorActionPreference = 'Continue'
    & $Run 2>&1 | ForEach-Object { "$_" }
    if ($LASTEXITCODE -ne 0) { throw "$What failed (exit $LASTEXITCODE)" }
}

if (-not (Test-Path $KeyPath)) { throw "Updater key not found at $KeyPath" }
$out = Join-Path $root 'target\packs'
New-Item -ItemType Directory -Force $out | Out-Null

$serial = [DateTime]::UtcNow.ToString('yyyyMMddHHmm')
$parts = @("# SnailNet packs (PLAN.md section 17.2), made by scripts\publish-packs.ps1.", 'format = 1', "serial = $serial")
# Bundle key = assets file. game_exes has a key of its own: builds up to
# 0.0.9 refuse unknown keys inside a pack (and drop the whole bundle), but
# ignore unknown bundle keys.
$packs = [ordered]@{ sites = 'sites'; families = 'families'; games = 'games'; game_exes = 'game-exes' }
foreach ($key in $packs.Keys) {
    $name = $packs[$key]
    $text = [IO.File]::ReadAllText((Join-Path $root "assets\$name.toml")) -replace "`r`n", "`n"
    if ($text.Contains("'''")) { throw "assets\$name.toml contains ''' and can't go in a literal string" }
    $parts += "$key = '''`n$text'''"
}
$body = ($parts -join "`n") + "`n"
$file = Join-Path $out 'packs.toml'
$utf8 = New-Object Text.UTF8Encoding $false
[IO.File]::WriteAllText($file, $body, $utf8)

Push-Location (Join-Path $root 'app')
try {
    # Through the environment, as build-installer.ps1 does: PowerShell 5.1
    # drops an empty '' argument on its way to a native command.
    $env:TAURI_SIGNING_PRIVATE_KEY = (Get-Content $KeyPath -Raw).Trim()
    if (-not $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD) { $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = '' }
    Invoke-Native 'signing' { npx tauri signer sign $file }
} finally { Pop-Location }
$b64 = (Get-Content "$file.sig" -Raw).Trim()
[IO.File]::WriteAllBytes("$file.minisig", [Convert]::FromBase64String($b64))
Remove-Item "$file.sig"

# The real key must accept it, and every pack in it must parse.
$env:SNAILNET_PACKS_DIR = $out
Invoke-Native 'checking the bundle' {
    cargo test -p snailnet-core --lib -- --ignored --exact packs::tests::a_published_bundle_checks_out --nocapture
}

[IO.File]::WriteAllText((Join-Path $root 'assets\packs-serial.txt'), "$serial`n", $utf8)
Write-Host "Bundle:    $file (serial $serial)"
Write-Host "Signature: $file.minisig"
Write-Host "assets\packs-serial.txt is now $($serial): commit it."

if ($Upload) {
    $ErrorActionPreference = 'Continue'
    gh release view packs *> $null
    $exists = $LASTEXITCODE -eq 0
    $ErrorActionPreference = 'Stop'
    if (-not $exists) {
        Invoke-Native "creating the release 'packs'" {
            gh release create packs --prerelease --latest=false --title 'Site and game lists' `
                --notes 'Signed site, app and game lists that SnailNet fetches once a day. Not an app release.'
        }
    }
    Invoke-Native 'uploading' { gh release upload packs $file "$file.minisig" --clobber }
    Write-Host "Uploaded to the release 'packs'."
}
