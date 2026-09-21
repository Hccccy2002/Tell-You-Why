param([switch]$Live)

$ErrorActionPreference = 'Stop'
$repoPath = Split-Path -Parent $PSScriptRoot
Push-Location -LiteralPath $repoPath
try {
    & node --test evals/search-agent/contract.test.mjs
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    if (-not $Live) { exit 0 }

    $previousSearchKey = $env:ZHIPU_API_KEY
    try {
        if (-not $env:ZHIPU_API_KEY) {
            $env:ZHIPU_API_KEY = [Environment]::GetEnvironmentVariable('ZHIPU_API_KEY', 'User')
        }
        & node evals/search-agent/probe.mjs
        $probeExitCode = $LASTEXITCODE
    }
    finally {
        $env:ZHIPU_API_KEY = $previousSearchKey
    }
    exit $probeExitCode
}
finally {
    Pop-Location
}
