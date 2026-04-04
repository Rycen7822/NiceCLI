param(
    [string]$WorkspaceRoot,
    [switch]$SkipNpmInstall
)

$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($WorkspaceRoot)) {
    $WorkspaceRoot = $PSScriptRoot
}

$scriptPath = Join-Path $PSScriptRoot "scripts\build-windows.ps1"
& $scriptPath -WorkspaceRoot $WorkspaceRoot -SkipNpmInstall:$SkipNpmInstall
