param(
    [string]$WorkspaceRoot = "D:\dev\wincli",
    [switch]$SkipNpmInstall
)

$ErrorActionPreference = "Stop"

$cliProxyRoot = Join-Path $WorkspaceRoot "CLIProxyAPI-6.9.7"
$easyCliRoot = Join-Path $WorkspaceRoot "EasyCLI-0.1.32"
$bundledRoot = Join-Path $easyCliRoot "src-tauri\bundled\cliproxyapi"
$targetVersion = [regex]::Match((Split-Path $cliProxyRoot -Leaf), '^CLIProxyAPI-(.+)$').Groups[1].Value

if ([string]::IsNullOrWhiteSpace($targetVersion)) {
    throw "Failed to infer CLIProxyAPI version from $cliProxyRoot"
}

$cliOutputDir = Join-Path $WorkspaceRoot "dist\cliproxyapi\$targetVersion"
$cliExePath = Join-Path $cliOutputDir "cli-proxy-api.exe"
$bundleVersionDir = Join-Path $bundledRoot $targetVersion
$buildDate = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")

Write-Host "Building CLIProxyAPI $targetVersion"
New-Item -ItemType Directory -Force -Path $cliOutputDir | Out-Null
Push-Location $cliProxyRoot
try {
    & go build `
        -trimpath `
        -ldflags "-s -w -X main.Version=$targetVersion -X main.Commit=local -X main.BuildDate=$buildDate" `
        -o $cliExePath `
        ./cmd/server
}
finally {
    Pop-Location
}

Write-Host "Preparing bundled CLIProxyAPI resources"
if (Test-Path $bundledRoot) {
    Remove-Item -Recurse -Force $bundledRoot
}
New-Item -ItemType Directory -Force -Path $bundleVersionDir | Out-Null
Copy-Item $cliExePath (Join-Path $bundleVersionDir "cli-proxy-api.exe")
Copy-Item (Join-Path $cliProxyRoot "config.example.yaml") (Join-Path $bundleVersionDir "config.example.yaml")
Set-Content -NoNewline -Path (Join-Path $bundledRoot "version.txt") -Value $targetVersion

Write-Host "Preparing frontend assets"
Push-Location $easyCliRoot
try {
    if (-not $SkipNpmInstall -or -not (Test-Path (Join-Path $easyCliRoot "node_modules"))) {
        & npm ci
    }
    & node src-tauri/prepare-web.js
}
finally {
    Pop-Location
}

Write-Host "Building nicecli.exe"
Push-Location (Join-Path $easyCliRoot "src-tauri")
try {
    & cargo build --release --bin nicecli
}
finally {
    Pop-Location
}

$devExe = Join-Path $easyCliRoot "src-tauri\target\release\nicecli.exe"

Write-Host ""
Write-Host "Build completed."
Write-Host "CLIProxyAPI exe: $cliExePath"
Write-Host "Bundled resources: $bundledRoot"
Write-Host "NiceCLI exe: $devExe"
