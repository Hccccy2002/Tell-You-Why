param([switch]$SkipDependencies)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$spec = Get-Content -LiteralPath (Join-Path $root 'packaging/python-runtime.json') -Raw | ConvertFrom-Json
$runtime = Join-Path $root 'src-tauri/pdf-runtime'
$downloads = Join-Path $root 'tmp/release-downloads'
New-Item -ItemType Directory -Force -Path $runtime,$downloads | Out-Null
$archive = Join-Path $downloads ('python-' + $spec.version + '-win64.tar.gz')
if (-not (Test-Path -LiteralPath $archive)) {
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    Invoke-WebRequest -UseBasicParsing -Uri $spec.url -OutFile $archive
}
if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash -ne $spec.sha256) { throw 'Python archive checksum failed. Remove the cached archive and retry.' }
$python = Join-Path $runtime 'python/python.exe'
if (-not (Test-Path -LiteralPath $python)) {
    & tar.exe -xzf $archive -C $runtime
    if ($LASTEXITCODE -ne 0) { throw 'Python extraction failed' }
}
if (-not $SkipDependencies) {
    & $python -m pip install --disable-pip-version-check -r (Join-Path $root 'rag-service/requirements.lock.txt')
    if ($LASTEXITCODE -ne 0) { throw 'PDF dependency installation failed' }
}
& $python -m pip check
if ($LASTEXITCODE -ne 0) { throw 'PDF dependency check failed' }
$env:PYTHONPATH = Join-Path $root 'rag-service/src'
$env:PYTHONNOUSERSITE = '1'
& $python (Join-Path $root 'packaging/prepare_runtime.py')
if ($LASTEXITCODE -ne 0) { throw 'Preparing packaged resources failed' }

# App-local Microsoft runtime: do not require a global VC++ installation.
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
$crt = $env:TELLWHY_MSVC_CRT
if (-not $crt -and (Test-Path -LiteralPath $vswhere)) {
    $vs = & $vswhere -latest -products '*' -property installationPath
    $redist = Get-ChildItem -LiteralPath (Join-Path $vs 'VC/Redist/MSVC') -Directory | Where-Object { $_.Name -match '^\d' } | Sort-Object Name -Descending | Select-Object -First 1
    if ($redist) { $crt = Join-Path $redist.FullName 'x64/Microsoft.VC143.CRT' }
}
if (-not $crt -or -not (Test-Path -LiteralPath (Join-Path $crt 'msvcp140.dll'))) { throw 'MSVC x64 redistributable files missing. Install VS C++ build tools or set TELLWHY_MSVC_CRT.' }
$crtDestination = Join-Path $root 'src-tauri/windows-crt'
New-Item -ItemType Directory -Force -Path $crtDestination | Out-Null
Get-ChildItem -LiteralPath $crt -Filter '*.dll' | ForEach-Object {
    Copy-Item -LiteralPath $_.FullName -Destination $crtDestination -Force
    Copy-Item -LiteralPath $_.FullName -Destination (Join-Path $runtime 'python') -Force
}
