[CmdletBinding()]
param(
    [string]$InstallDir = (Join-Path $env:LOCALAPPDATA "LanSaveSync"),
    [string]$ConfigPath = (Join-Path $env:APPDATA "LanSaveSync\agent.json"),
    [string]$DeviceId = "windows-pc",
    [string]$DeviceName = $env:COMPUTERNAME
)

$ErrorActionPreference = "Stop"
$sourceBinary = Join-Path $PSScriptRoot "lan-save-sync.exe"
if (-not (Test-Path -LiteralPath $sourceBinary -PathType Leaf)) {
    throw "lan-save-sync.exe must be placed next to install.ps1"
}

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
$installedBinary = Join-Path $InstallDir "lan-save-sync.exe"
Copy-Item -LiteralPath $sourceBinary -Destination $installedBinary -Force

if (-not (Test-Path -LiteralPath $ConfigPath -PathType Leaf)) {
    & $installedBinary init `
        --device-id $DeviceId `
        --name $DeviceName `
        --output $ConfigPath
    Write-Host "Edit the new configuration before the Agent can sync folders:"
    Write-Host "  $ConfigPath"
}

$runKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run"
New-Item -Path $runKey -Force | Out-Null
$command = "`"$installedBinary`" --config `"$ConfigPath`" serve"
New-ItemProperty `
    -Path $runKey `
    -Name "LanSaveSync" `
    -Value $command `
    -PropertyType String `
    -Force | Out-Null

Write-Host "LAN Save Sync installed for the current user."
Write-Host "It will start after the next sign-in. Run it now after configuring peers and folders:"
Write-Host "  $command"
Write-Host "Windows may ask you to allow access on Private networks."
