param([switch]$SkipRuntimePreparation, [switch]$SkipAppBuild)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $root
foreach ($command in 'node','npm.cmd','cargo') {
    if (-not (Get-Command $command -ErrorAction SilentlyContinue)) { throw "Missing $command; see README developer prerequisites." }
}
if (-not $SkipRuntimePreparation) { & "$PSScriptRoot/prepare-pdf-runtime.ps1" }
& "$PSScriptRoot/prepare-windows-tools.ps1"
& "$PSScriptRoot/enable-python-utf8.ps1"
if (-not $SkipAppBuild) {
    & npm.cmd ci
    if ($LASTEXITCODE -ne 0) { throw 'npm ci failed' }
    & npm.cmd run tauri -- build --no-bundle --config src-tauri/tauri.full.conf.json
    if ($LASTEXITCODE -ne 0) { throw 'Tauri build failed' }
}
$version = (Get-Content -LiteralPath 'package.json' -Raw | ConvertFrom-Json).version
$assetName = "Tell-You-Why_$($version)_windows-x64-full-setup.exe"
& (Join-Path $root 'tmp/build-tools/Inno Setup 7/ISCC.exe') /Qp ("/DAppVersion=" + $version) (Join-Path $root 'packaging/windows.iss')
if ($LASTEXITCODE -ne 0) { throw 'Inno Setup packaging failed' }
$destination = Join-Path (Join-Path $root 'release-artifacts') $assetName
if ((Get-Item -LiteralPath $destination).Length -ge 2GB) { throw 'Installer exceeds the GitHub single-asset limit.' }
$hash = (Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash.ToLowerInvariant()
$utf8 = New-Object System.Text.UTF8Encoding($false)
[IO.File]::WriteAllText((Join-Path $root 'release-artifacts/SHA256SUMS.txt'), "$hash  $assetName" + [Environment]::NewLine, $utf8)
$manifest = @{repository='Hccccy2002/Tell-You-Why';version=$version;tag="v$version";asset=$assetName;sha256=$hash;size=(Get-Item -LiteralPath $destination).Length}
[IO.File]::WriteAllText((Join-Path $root 'packaging/release.json'), ($manifest | ConvertTo-Json) + [Environment]::NewLine, $utf8)
Write-Host "Installer: $destination"
Write-Host "SHA256: $hash"
