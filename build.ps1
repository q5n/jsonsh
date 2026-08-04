[CmdletBinding()]
param(
    [switch]$SkipTests
)

$ErrorActionPreference = 'Stop'
$projectRoot = $PSScriptRoot
$outputDir = Join-Path $projectRoot 'dist'
$outputFile = Join-Path $outputDir 'jsonsh.exe'

try {
    $null = Get-Command go -ErrorAction Stop
} catch {
    throw '未找到 Go。请先安装 Go，并确保 go 命令已加入 PATH。'
}

Set-Location $projectRoot

if (-not $SkipTests) {
    Write-Host '[1/2] 正在运行测试...' -ForegroundColor Cyan
    & go test ./...
    if ($LASTEXITCODE -ne 0) {
        throw "测试失败，退出码：$LASTEXITCODE"
    }
} else {
    Write-Host '[1/2] 已跳过测试。' -ForegroundColor Yellow
}

Write-Host '[2/2] 正在构建 jsonsh...' -ForegroundColor Cyan
New-Item -ItemType Directory -Path $outputDir -Force | Out-Null
& go build -trimpath -o $outputFile ./cmd/jsonsh
if ($LASTEXITCODE -ne 0) {
    throw "构建失败，退出码：$LASTEXITCODE"
}

Write-Host ''
Write-Host '构建成功：' -ForegroundColor Green
Write-Host $outputFile
