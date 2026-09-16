param(
    [string]$InstallDirectory = (Join-Path $env:LOCALAPPDATA 'Programs\Tell You Why'),
    [switch]$NoLaunch,
    [switch]$CheckOnly
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$manifest = Get-Content -LiteralPath (Join-Path $root 'packaging/release.json') -Raw | ConvertFrom-Json
$exe = Join-Path $InstallDirectory 'tell-you-why.exe'
$marker = Join-Path $InstallDirectory 'tellwhy-release.json'
$releasePage = 'https://github.com/' + $manifest.repository + '/releases/tag/' + $manifest.tag
$installed = $false
if ((Test-Path -LiteralPath $exe) -and (Test-Path -LiteralPath $marker)) {
    $previous = Get-Content -LiteralPath $marker -Raw | ConvertFrom-Json
    $installed = $previous.sha256 -eq $manifest.sha256 -and
        (Test-Path -LiteralPath (Join-Path $InstallDirectory 'pdf-runtime/python/python.exe'))
}
if ($CheckOnly) {
    [pscustomobject]@{version=$manifest.version;installed=$installed;exe=$exe;release=$releasePage}
    exit 0
}

# Avoid depending on Get-FileHash module auto-loading in Windows PowerShell.
function Get-InstallerSha256([string]$path) {
    $stream = [IO.File]::OpenRead($path)
    $algorithm = [Security.Cryptography.SHA256]::Create()
    try { return [BitConverter]::ToString($algorithm.ComputeHash($stream)).Replace('-','').ToLowerInvariant() }
    finally { $algorithm.Dispose(); $stream.Dispose() }
}

function Get-GitHubToken {
    if ($env:GH_TOKEN) { return $env:GH_TOKEN }
    if ($env:GITHUB_TOKEN) { return $env:GITHUB_TOKEN }
    if (Get-Command gh -ErrorAction SilentlyContinue) {
        $value = & gh auth token --hostname github.com 2>$null
        if ($LASTEXITCODE -eq 0 -and $value) { return ($value -join '').Trim() }
    }
    if (Get-Command git -ErrorAction SilentlyContinue) {
        $oldPrompt = $env:GIT_TERMINAL_PROMPT
        $oldInteractive = $env:GCM_INTERACTIVE
        try {
            $env:GIT_TERMINAL_PROMPT = '0'
            $env:GCM_INTERACTIVE = 'Never'
            $request = "protocol=https" + [char]10 + "host=github.com" + [char]10 + [char]10
            $lines = $request | & git credential fill 2>$null
            if ($LASTEXITCODE -eq 0) {
                foreach ($line in $lines) { if ($line -match '^password=(.+)$') { return $matches[1] } }
            }
        } finally { $env:GIT_TERMINAL_PROMPT=$oldPrompt; $env:GCM_INTERACTIVE=$oldInteractive }
    }
    return $null
}

function Download-Installer($destination) {
    $token = Get-GitHubToken
    if (-not $token) {
        throw "Private repository: log in once with 'gh auth login', or download the installer from $releasePage and place it beside start.cmd."
    }
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    $headers = @{Authorization="Bearer $token";Accept='application/vnd.github+json';'User-Agent'='TellYouWhy-Launcher'}
    $release = Invoke-RestMethod -Uri ('https://api.github.com/repos/' + $manifest.repository + '/releases/tags/' + $manifest.tag) -Headers $headers
    $assets = @($release.assets | Where-Object { $_.name -eq $manifest.asset })
    if ($assets.Count -ne 1) { throw 'The pinned installer is missing from this GitHub release.' }
    if ($assets[0].size -ne $manifest.size) { throw 'Release asset size differs from the source manifest.' }
    Add-Type -AssemblyName System.Net.Http
    $handler = New-Object System.Net.Http.HttpClientHandler
    $handler.AllowAutoRedirect = $false
    $client = New-Object System.Net.Http.HttpClient($handler)
    $client.Timeout = [TimeSpan]::FromHours(2)
    try {
        $request = New-Object System.Net.Http.HttpRequestMessage([System.Net.Http.HttpMethod]::Get, $assets[0].url)
        $request.Headers.Add('User-Agent','TellYouWhy-Launcher')
        $request.Headers.Add('Authorization',"Bearer $token")
        $request.Headers.Add('Accept','application/octet-stream')
        $response = $client.SendAsync($request,[System.Net.Http.HttpCompletionOption]::ResponseHeadersRead).GetAwaiter().GetResult()
        if ([int]$response.StatusCode -ge 300 -and [int]$response.StatusCode -lt 400) {
            $url = $response.Headers.Location
            if ($url.Scheme -ne 'https' -or -not $url.Host.EndsWith('.githubusercontent.com')) {
                throw 'Unexpected GitHub download redirect.'
            }
            $response.Dispose()
            # The GitHub token is never forwarded to the asset/CDN host.
            $response = $client.GetAsync($url,[System.Net.Http.HttpCompletionOption]::ResponseHeadersRead).GetAwaiter().GetResult()
        }
        $response.EnsureSuccessStatusCode() | Out-Null
        $file = [IO.File]::Create($destination)
        try { $response.Content.CopyToAsync($file).GetAwaiter().GetResult() } finally { $file.Dispose() }
        $response.Dispose()
    } finally { $client.Dispose(); $handler.Dispose() }
}

if (-not $installed) {
    $cache = Join-Path $env:LOCALAPPDATA ('TellYouWhy-Installer/' + $manifest.version)
    New-Item -ItemType Directory -Force -Path $cache | Out-Null
    $installer = Join-Path $cache $manifest.asset
    $localAsset = Join-Path $root $manifest.asset
    if (Test-Path -LiteralPath $localAsset) { $installer=$localAsset }
    if (-not (Test-Path -LiteralPath $installer)) {
        Write-Host "Downloading the full PDF installer. This may take several minutes..."
        $partial = $installer + '.partial'
        # Recover a fully downloaded file if a prior run stopped before installation.
        $complete = (Test-Path -LiteralPath $partial) -and
            (Get-Item -LiteralPath $partial).Length -eq $manifest.size -and
            (Get-InstallerSha256 $partial) -eq $manifest.sha256
        if ($complete) { Write-Host 'Reusing the completed, verified download.' }
        else { Download-Installer $partial }
        if ((Get-InstallerSha256 $partial) -ne $manifest.sha256) { throw 'Installer checksum failed; it will not be executed.' }
        Move-Item -LiteralPath $partial -Destination $installer -Force
    }
    if ((Get-InstallerSha256 $installer) -ne $manifest.sha256) { throw 'Installer checksum failed; it will not be executed. Download the release again.' }
    # Require a normal exit before updating files in the selected installation.
    if (Get-Process -Name 'tell-you-why' -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $exe }) {
        throw 'Tell You Why is running. Exit it from the tray menu, then run start.cmd again.'
    }
    Write-Host "Installing Tell You Why $($manifest.version)..."
    $install = Start-Process -FilePath $installer -ArgumentList @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART','/SP-',('/DIR="' + $InstallDirectory + '"')) -WindowStyle Hidden -PassThru -Wait
    if ($install.ExitCode -notin @(0,3010)) { throw "Installer failed with exit code $($install.ExitCode)." }
    if (-not (Test-Path -LiteralPath $exe)) { throw 'Installation finished without the expected executable.' }
    Copy-Item -LiteralPath (Join-Path $root 'packaging/release.json') -Destination $marker -Force
}
if (-not $NoLaunch) { Start-Process -FilePath $exe -WorkingDirectory $InstallDirectory }
Write-Host "Ready: $exe"
