$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$kits = (Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows Kits\Installed Roots' -ErrorAction SilentlyContinue).KitsRoot10
if (-not $kits) { $kits = Join-Path ([Environment]::GetFolderPath('ProgramFilesX86')) 'Windows Kits/10' }
$tool = Get-ChildItem -LiteralPath (Join-Path $kits 'bin') -Directory |
    Where-Object { $_.Name -match '^\d+\.\d+\.\d+\.\d+$' } |
    Sort-Object { [version]$_.Name } -Descending |
    ForEach-Object { Join-Path $_.FullName 'x64/mt.exe' } |
    Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
if (-not $tool) { throw 'Windows SDK mt.exe is required to enable Unicode paths in the PDF runtime.' }
$cache = Join-Path $root 'tmp/python-manifests'
New-Item -ItemType Directory -Force -Path $cache | Out-Null
foreach ($name in 'python.exe','pythonw.exe') {
    $exe = Join-Path (Join-Path $root 'src-tauri/pdf-runtime/python') $name
    $manifest = Join-Path $cache ($name + '.manifest')
    & $tool -nologo ('-inputresource:' + $exe + ';#1') ('-out:' + $manifest)
    if ($LASTEXITCODE -ne 0) { throw "Cannot read Python manifest: $name" }
    $xml = [IO.File]::ReadAllText($manifest)
    if ($xml -notmatch '<activeCodePage') {
        if (-not $xml.Contains('<windowsSettings>')) { throw 'Unexpected Python application manifest' }
        $xml = $xml.Replace('<windowsSettings>','<windowsSettings><activeCodePage xmlns="http://schemas.microsoft.com/SMI/2019/WindowsSettings">UTF-8</activeCodePage>')
        [IO.File]::WriteAllText($manifest,$xml,(New-Object System.Text.UTF8Encoding($false)))
        & $tool -nologo -manifest $manifest ('-outputresource:' + $exe + ';#1')
        if ($LASTEXITCODE -ne 0) { throw "Cannot set Python UTF-8 manifest: $name" }
    } elseif ($xml -notmatch '>UTF-8</activeCodePage>') {
        throw 'Unexpected Python active code page'
    }
}
& (Join-Path $root 'src-tauri/pdf-runtime/python/python.exe') -c 'import ctypes; assert ctypes.windll.kernel32.GetACP() == 65001, "Windows 10 1903 or later is required"'
if ($LASTEXITCODE -ne 0) { throw 'UTF-8 process code page check failed' }
