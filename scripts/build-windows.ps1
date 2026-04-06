param(
    [string]$WorkspaceRoot,
    [switch]$SkipNpmInstall
)

$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($WorkspaceRoot)) {
    $WorkspaceRoot = Split-Path $PSScriptRoot -Parent
}

$niceCliRoot = Join-Path $WorkspaceRoot "apps\nicecli"
if (-not (Test-Path $niceCliRoot)) {
    throw "NiceCLI app directory not found: $niceCliRoot"
}
Write-Host "Preparing frontend assets"
Push-Location $niceCliRoot
try {
    if (-not $SkipNpmInstall -or -not (Test-Path (Join-Path $niceCliRoot "node_modules"))) {
        & npm ci
        if ($LASTEXITCODE -ne 0) {
            throw "npm ci failed"
        }
    }
    & npm run build
    if ($LASTEXITCODE -ne 0) {
        throw "npm run build failed"
    }
}
finally {
    Pop-Location
}

$releaseExe = Join-Path $niceCliRoot "src-tauri\target\release\nicecli.exe"

Write-Host ""
Write-Host "Build completed."
Write-Host "NiceCLI exe: $releaseExe"
