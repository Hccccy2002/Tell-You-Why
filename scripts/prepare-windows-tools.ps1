$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$spec = Get-Content -LiteralPath (Join-Path $root 'packaging/windows-tools.json') -Raw | ConvertFrom-Json
$downloads = Join-Path $root 'tmp/release-downloads'
New-Item -ItemType Directory -Force -Path $downloads | Out-Null
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
function Get-VerifiedTool($item, $path, $publisher) {
    if (-not (Test-Path -LiteralPath $path)) { Invoke-WebRequest -UseBasicParsing -Uri $item.url -OutFile $path }
    if ((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash -ne $item.sha256) { throw "Tool checksum failed: $path" }
    $signature = Get-AuthenticodeSignature -LiteralPath $path
    if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notmatch $publisher) { throw "Tool publisher verification failed: $path" }
}
$installer = Join-Path $downloads ('innosetup-' + $spec.inno.version + '-x64.exe')
Get-VerifiedTool $spec.inno $installer 'Pyrsys'
$compilerDir = Join-Path $root 'tmp/build-tools/Inno Setup 7'
if (-not (Test-Path -LiteralPath (Join-Path $compilerDir 'ISCC.exe'))) {
    $process = Start-Process -FilePath $installer -ArgumentList @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART','/CURRENTUSER',('/DIR="' + $compilerDir + '"')) -WindowStyle Hidden -Wait -PassThru
    if ($process.ExitCode -ne 0) { throw 'Inno Setup compiler installation failed' }
}
Get-VerifiedTool $spec.webview2 (Join-Path $downloads 'WebView2-offline.exe') 'Microsoft Corporation'
