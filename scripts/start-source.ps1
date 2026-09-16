$ErrorActionPreference = 'Stop'
Set-Location -LiteralPath (Split-Path -Parent $PSScriptRoot)
foreach ($command in 'node','npm.cmd','cargo') {
    if (-not (Get-Command $command -ErrorAction SilentlyContinue)) { throw "Missing $command; see README developer prerequisites." }
}
if (-not (Test-Path -LiteralPath 'node_modules')) {
    & npm.cmd ci
    if ($LASTEXITCODE -ne 0) { throw 'npm ci failed' }
}
& npm.cmd run tauri -- dev
if ($LASTEXITCODE -ne 0) { throw 'Source startup failed' }
