[CmdletBinding()]
param(
    [switch]$SkipTests,
    [string]$Version
)

$ErrorActionPreference = 'Stop'
$projectRoot = $PSScriptRoot
$outputDir = Join-Path $projectRoot 'dist'
$outputFile = Join-Path $outputDir 'jsonsh.exe'

try {
    $null = Get-Command go -ErrorAction Stop
} catch {
    throw 'Go was not found. Install Go and make sure the go command is available in PATH.'
}

Set-Location $projectRoot

if ([string]::IsNullOrWhiteSpace($Version)) {
    try {
        $Version = (& git describe --tags --always --dirty 2>$null).Trim()
    } catch {
        $Version = 'dev'
    }
    if ([string]::IsNullOrWhiteSpace($Version)) {
        $Version = 'dev'
    }
}

if (-not $SkipTests) {
    Write-Host '[1/2] Running tests...' -ForegroundColor Cyan
    & go test ./...
    if ($LASTEXITCODE -ne 0) {
        throw "Tests failed with exit code $LASTEXITCODE."
    }
} else {
    Write-Host '[1/2] Tests skipped.' -ForegroundColor Yellow
}

Write-Host '[2/2] Building jsonsh...' -ForegroundColor Cyan
New-Item -ItemType Directory -Path $outputDir -Force | Out-Null
& go build -trimpath -ldflags "-X main.version=$Version" -o $outputFile ./cmd/jsonsh
if ($LASTEXITCODE -ne 0) {
    throw "Build failed with exit code $LASTEXITCODE."
}

Write-Host ''
Write-Host 'Build succeeded:' -ForegroundColor Green
Write-Host $outputFile
Write-Host "Version: $Version"
