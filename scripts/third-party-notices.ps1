param(
    [string]$Out = ''
)
# Writes THIRD-PARTY-NOTICES.txt (PLAN.md §16, §21 Phase 6): every crate
# that ships inside snailnet-app (SnailNet.exe) or snailnet-service, by
# following normal (not dev or build) dependencies for Windows x64, plus the
# npm packages bundled into the app's web page. For each: name, version,
# licence expression, and the licence and notice files the package ships.
# WinDivert has its own file next to it (licenses\WinDivert-LICENSE.txt).
#
# scripts\build-installer.ps1 runs this before bundling, into the cargo
# target folder (it isn't committed: ~3 MB that changes with every
# dependency bump); the installer puts it in the install folder's licenses\.
$ErrorActionPreference = 'Stop'
$root = Resolve-Path (Join-Path $PSScriptRoot '..')
if (-not $Out) { $Out = Join-Path $root 'target\THIRD-PARTY-NOTICES.txt' }
New-Item -ItemType Directory -Force (Split-Path $Out) | Out-Null

Push-Location $root
try {
    $json = cargo metadata --format-version 1 --locked --filter-platform x86_64-pc-windows-msvc
    if ($LASTEXITCODE -ne 0) { throw "cargo metadata failed (exit $LASTEXITCODE)" }
} finally { Pop-Location }
$meta = $json | ConvertFrom-Json

$byId = @{}
foreach ($p in $meta.packages) { $byId[$p.id] = $p }
$nodes = @{}
foreach ($n in $meta.resolve.nodes) { $nodes[$n.id] = $n }
$workspace = @{}
foreach ($id in $meta.workspace_members) { $workspace[$id] = $true }

# Walk normal dependencies from the two shipped binaries.
$roots = $meta.packages | Where-Object { $_.name -in @('snailnet-app', 'snailnet-service') -and $workspace[$_.id] }
if (@($roots).Count -ne 2) { throw 'snailnet-app or snailnet-service not found in the workspace' }
$seen = @{}
$queue = New-Object System.Collections.Queue
foreach ($r in $roots) { $queue.Enqueue($r.id) }
while ($queue.Count -gt 0) {
    $id = $queue.Dequeue()
    if ($seen[$id]) { continue }
    $seen[$id] = $true
    foreach ($d in $nodes[$id].deps) {
        $normal = $d.dep_kinds | Where-Object { $null -eq $_.kind }
        if ($normal) { $queue.Enqueue($d.pkg) }
    }
}

$licencePattern = '^(LICEN[CS]E|COPYING|NOTICE|COPYRIGHT|UNLICENSE)'
function Get-LicenceTexts([string]$Dir) {
    if (-not (Test-Path $Dir)) { return @() }
    Get-ChildItem -LiteralPath $Dir -File |
        Where-Object { $_.Name -match $licencePattern } |
        Sort-Object Name |
        ForEach-Object { [pscustomobject]@{ Name = $_.Name; Text = (Get-Content -LiteralPath $_.FullName -Raw) } }
}

$entries = New-Object System.Collections.Generic.List[object]
foreach ($id in $seen.Keys) {
    if ($workspace[$id]) { continue }
    $p = $byId[$id]
    $dir = Split-Path $p.manifest_path
    $entries.Add([pscustomobject]@{
            Name    = $p.name
            Version = $p.version
            License = if ($p.license) { $p.license } elseif ($p.license_file) { "see $($p.license_file)" } else { 'unknown' }
            Url     = $p.repository
            Authors = @($p.authors) -join ', '
            Texts   = @(Get-LicenceTexts $dir)
        })
}

# The npm packages whose code or files end up in the bundled page.
$npm = @('@tauri-apps/api', '@fontsource-variable/atkinson-hyperlegible-next')
foreach ($name in $npm) {
    $dir = Join-Path $root "app\node_modules\$name"
    $pkgFile = Join-Path $dir 'package.json'
    if (-not (Test-Path $pkgFile)) { throw "$name isn't installed (run npm ci in app\)" }
    $pkg = Get-Content $pkgFile -Raw | ConvertFrom-Json
    $url = if ($pkg.repository -is [string]) { $pkg.repository } elseif ($pkg.repository) { $pkg.repository.url } else { $null }
    $entries.Add([pscustomobject]@{
            Name    = $name
            Version = $pkg.version
            License = $pkg.license
            Url     = $url
            Authors = if ($pkg.author -is [string]) { $pkg.author } elseif ($pkg.author) { $pkg.author.name } else { '' }
            Texts   = @(Get-LicenceTexts $dir)
        })
}

$missing = @($entries | Where-Object { $_.Texts.Count -eq 0 })
$sb = New-Object System.Text.StringBuilder
[void]$sb.AppendLine('SnailNet: third-party notices')
[void]$sb.AppendLine('')
[void]$sb.AppendLine('SnailNet includes the software listed below. Each is used under the licence')
[void]$sb.AppendLine('named, and its licence and notice files follow. WinDivert (LGPLv3 or GPLv2)')
[void]$sb.AppendLine('ships unmodified as WinDivert.dll and WinDivert64.sys; its licence is in')
[void]$sb.AppendLine('WinDivert-LICENSE.txt in this folder and its source is at')
[void]$sb.AppendLine('https://github.com/basil00/WinDivert/releases/tag/v2.2.2.')
[void]$sb.AppendLine('')
foreach ($e in ($entries | Sort-Object Name, Version)) {
    [void]$sb.AppendLine(('=' * 78))
    [void]$sb.AppendLine("$($e.Name) $($e.Version)")
    [void]$sb.AppendLine("Licence: $($e.License)")
    if ($e.Url) { [void]$sb.AppendLine("Source: $($e.Url)") }
    foreach ($t in $e.Texts) {
        [void]$sb.AppendLine('')
        [void]$sb.AppendLine("--- $($t.Name)")
        [void]$sb.AppendLine($t.Text.TrimEnd())
    }
    if ($e.Texts.Count -eq 0) {
        [void]$sb.AppendLine('')
        if ($e.Authors) { [void]$sb.AppendLine("Copyright (c) $($e.Authors)") }
        [void]$sb.AppendLine("(No licence file in the package; the standard $($e.License) text applies.)")
    }
    [void]$sb.AppendLine('')
}
# CRLF and UTF-8 without a BOM: readable in Notepad, stable in git.
$text = $sb.ToString() -replace "`r?`n", "`r`n"
[IO.File]::WriteAllText($Out, $text, (New-Object Text.UTF8Encoding $false))
Write-Host "Wrote $Out ($($entries.Count) packages, $($missing.Count) without a licence file)"
