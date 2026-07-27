[CmdletBinding()]
param(
    [string]$DeviceId = "portable-pc",
    [string]$DeviceName = $env:COMPUTERNAME
)

$ErrorActionPreference = "Stop"
$binary = Join-Path $PSScriptRoot "lan-save-sync.exe"
$config = Join-Path $PSScriptRoot "agent.json"
if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) {
    throw "lan-save-sync.exe must be placed next to run-portable.ps1"
}
if (-not (Test-Path -LiteralPath $config -PathType Leaf)) {
    & $binary init --device-id $DeviceId --name $DeviceName --output $config
    Write-Host "Created $config. Add peers and folders, then run this script again."
    exit 0
}
& $binary --config $config serve
