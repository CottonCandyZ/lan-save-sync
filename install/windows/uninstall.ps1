[CmdletBinding()]
param(
    [string]$InstallDir = (Join-Path $env:LOCALAPPDATA "LanSaveSync"),
    [string]$ConfigDir = (Join-Path $env:APPDATA "LanSaveSync"),
    [switch]$RemoveData
)

$ErrorActionPreference = "Stop"
$runKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run"
Remove-ItemProperty -Path $runKey -Name "LanSaveSync" -ErrorAction SilentlyContinue

$installedBinary = [System.IO.Path]::GetFullPath(
    (Join-Path $InstallDir "lan-save-sync.exe")
)
Get-CimInstance Win32_Process |
    Where-Object {
        $_.ExecutablePath -and
        ([System.IO.Path]::GetFullPath($_.ExecutablePath) -eq $installedBinary)
    } |
    ForEach-Object { Stop-Process -Id $_.ProcessId -Force }

$resolvedInstall = [System.IO.Path]::GetFullPath($InstallDir)
$localAppData = [System.IO.Path]::GetFullPath($env:LOCALAPPDATA)
if (-not $resolvedInstall.StartsWith($localAppData, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Refusing to remove an install directory outside LOCALAPPDATA: $resolvedInstall"
}
if (Test-Path -LiteralPath $resolvedInstall) {
    Remove-Item -LiteralPath $resolvedInstall -Recurse -Force
}

if ($RemoveData) {
    $resolvedConfig = [System.IO.Path]::GetFullPath($ConfigDir)
    $appData = [System.IO.Path]::GetFullPath($env:APPDATA)
    if (-not $resolvedConfig.StartsWith($appData, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing to remove a config directory outside APPDATA: $resolvedConfig"
    }
    if (Test-Path -LiteralPath $resolvedConfig) {
        Remove-Item -LiteralPath $resolvedConfig -Recurse -Force
    }
    Write-Host "Program, configuration, and local version history removed."
} else {
    Write-Host "Program removed. Configuration and history were kept at $ConfigDir"
    Write-Host "Run again with -RemoveData to delete them too."
}
