<#
.SYNOPSIS
Installs a verified Shu release for the current Windows user.

.PARAMETER Version
A release tag or version, such as v0.1.0. The default is latest.

.PARAMETER InstallDir
The directory that receives shu.exe. The default is %LOCALAPPDATA%\shu\bin.
#>
[CmdletBinding()]
param(
    [string]$Version = $env:SHU_VERSION,
    [string]$InstallDir = $env:SHU_INSTALL_DIR,
    [string]$Repository = $env:SHU_INSTALL_REPO,
    [switch]$NoPathUpdate
)

$ErrorActionPreference = 'Stop'
if ([string]::IsNullOrWhiteSpace($Version)) { $Version = 'latest' }
if ([string]::IsNullOrWhiteSpace($InstallDir)) { $InstallDir = Join-Path $env:LOCALAPPDATA 'shu\bin' }
if ([string]::IsNullOrWhiteSpace($Repository)) { $Repository = 'wiedymi/shu' }

$asset = 'shu-x86_64-pc-windows-msvc.zip'
if ($Version -eq 'latest') {
    $downloadBase = "https://github.com/$Repository/releases/latest/download"
} else {
    if (-not $Version.StartsWith('v')) { $Version = "v$Version" }
    $downloadBase = "https://github.com/$Repository/releases/download/$Version"
}

$temporaryDirectory = Join-Path ([System.IO.Path]::GetTempPath()) ("shu-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $temporaryDirectory -Force | Out-Null
try {
    $archive = Join-Path $temporaryDirectory $asset
    $checksums = Join-Path $temporaryDirectory 'SHA256SUMS'
    Invoke-WebRequest -Uri "$downloadBase/$asset" -OutFile $archive
    Invoke-WebRequest -Uri "$downloadBase/SHA256SUMS" -OutFile $checksums

    $checksumLine = Get-Content $checksums | Where-Object { $_ -match ("\*?" + [regex]::Escape($asset) + '$') } | Select-Object -First 1
    if ([string]::IsNullOrWhiteSpace($checksumLine)) { throw "$asset was not listed in SHA256SUMS" }
    $expected = ($checksumLine -split '\s+')[0].ToLowerInvariant()
    $actual = (Get-FileHash -Algorithm SHA256 -Path $archive).Hash.ToLowerInvariant()
    if ($expected -ne $actual) { throw "Checksum verification failed for $asset" }

    Expand-Archive -Path $archive -DestinationPath $temporaryDirectory -Force
    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    Copy-Item (Join-Path $temporaryDirectory 'shu.exe') (Join-Path $InstallDir 'shu.exe') -Force
} finally {
    Remove-Item -LiteralPath $temporaryDirectory -Recurse -Force -ErrorAction SilentlyContinue
}

$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if (-not $NoPathUpdate -and ($userPath -split ';' | Where-Object { $_ -eq $InstallDir }).Count -eq 0) {
    $newPath = @($userPath, $InstallDir) | Where-Object { -not [string]::IsNullOrWhiteSpace($_) } | Join-String -Separator ';'
    [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
    $pathNotice = ' Added it to your user PATH; open a new terminal before using shu.'
} else {
    $pathNotice = ''
}
Write-Host "Installed Shu to $(Join-Path $InstallDir 'shu.exe').$pathNotice"
