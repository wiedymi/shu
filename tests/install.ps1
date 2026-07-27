<#
.SYNOPSIS
Exercises the PowerShell release installer without network access or user PATH changes.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string]$InstallerPath,
    [Parameter(Mandatory)]
    [string]$BinaryPath
)

$ErrorActionPreference = 'Stop'
$workspace = Join-Path ([System.IO.Path]::GetTempPath()) ("shu-installer-test-" + [guid]::NewGuid())
try {
    $assets = Join-Path $workspace 'assets'
    $payload = Join-Path $workspace 'payload'
    $installed = Join-Path $workspace 'installed'
    $nestedBinaryDirectory = Join-Path $payload 'target\x86_64-pc-windows-msvc\release'
    New-Item -ItemType Directory -Path $assets, $nestedBinaryDirectory -Force | Out-Null
    Copy-Item $BinaryPath (Join-Path $nestedBinaryDirectory 'shu.exe')
    $asset = 'shu-x86_64-pc-windows-msvc.zip'
    $archive = Join-Path $assets $asset
    Compress-Archive -Path (Join-Path $payload 'target') -DestinationPath $archive
    $hash = (Get-FileHash -Algorithm SHA256 -Path $archive).Hash.ToLowerInvariant()
    Set-Content -Path (Join-Path $assets 'SHA256SUMS') -Value "$hash  $asset"

    function global:Invoke-WebRequest {
        param([string]$Uri, [string]$OutFile)
        $name = [System.IO.Path]::GetFileName(([uri]$Uri).AbsolutePath)
        Copy-Item (Join-Path $assets $name) $OutFile
    }

    & $InstallerPath -Version vtest -InstallDir $installed -Repository example/shu -NoPathUpdate
    & (Join-Path $installed 'shu.exe') --version | Select-String '^shu ' | Out-Null
    if (-not (Test-Path (Join-Path $installed 'shu.exe'))) {
        throw 'installer did not place shu.exe in the selected directory'
    }
} finally {
    Remove-Item Function:\global:Invoke-WebRequest -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $workspace -Recurse -Force -ErrorAction SilentlyContinue
}
