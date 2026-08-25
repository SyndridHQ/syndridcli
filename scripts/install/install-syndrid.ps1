param(
    [string]$Release = $env:SYNDRID_RELEASE,
    [string]$InstallDir = $env:SYNDRID_INSTALL_DIR,
    [string]$Repository = $env:SYNDRID_GITHUB_REPOSITORY,
    [string]$SyndridHome = $env:SYNDRID_HOME
)

$ErrorActionPreference = 'Stop'
if (-not $Release) { $Release = 'latest' }
if (-not $InstallDir) { $InstallDir = Join-Path $HOME '.local\bin' }
if (-not $Repository) { $Repository = 'SyndridHQ/syndridcli' }
if (-not $SyndridHome) { $SyndridHome = Join-Path $HOME '.syndrid' }

$osArch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
switch ($osArch) {
    'X64' { $arch = 'x86_64' }
    'Arm64' { $arch = 'aarch64' }
    default { throw "Unsupported Windows architecture: $osArch" }
}
$target = "$arch-pc-windows-msvc"

if ($Release -eq 'latest') {
    $metadata = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repository/releases/latest"
    $tag = $metadata.tag_name
    if (-not $tag) { throw 'Could not resolve latest Syndrid release tag.' }
} elseif ($Release.StartsWith('rust-v')) {
    $tag = $Release
} elseif ($Release.StartsWith('v')) {
    $tag = "rust-$Release"
} else {
    $tag = "rust-v$Release"
}

$base = "https://github.com/$Repository/releases/download/$tag"
$asset = "syndrid-package-$target.tar.gz"
$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("syndrid-install-" + [Guid]::NewGuid())
$standaloneRoot = Join-Path $SyndridHome 'packages\standalone'
$releasesDir = Join-Path $standaloneRoot 'releases'
$currentDir = Join-Path $standaloneRoot 'current'
New-Item -ItemType Directory -Path $tmp | Out-Null

try {
    $manifest = Join-Path $tmp 'syndrid-package_SHA256SUMS'
    $archive = Join-Path $tmp $asset
    Invoke-WebRequest -UseBasicParsing -Uri "$base/syndrid-package_SHA256SUMS" -OutFile $manifest
    Invoke-WebRequest -UseBasicParsing -Uri "$base/$asset" -OutFile $archive

    $expected = $null
    foreach ($line in Get-Content $manifest) {
        if ($line -match '^([0-9a-fA-F]{64})\s+(.+)$' -and $Matches[2] -eq $asset) {
            $expected = $Matches[1].ToLowerInvariant()
            break
        }
    }
    if (-not $expected) { throw "$asset is missing from syndrid-package_SHA256SUMS." }
    $actual = (Get-FileHash -Algorithm SHA256 -Path $archive).Hash.ToLowerInvariant()
    if ($actual -ne $expected) { throw "Checksum mismatch for $asset." }

    $packageDir = Join-Path $tmp 'package'
    New-Item -ItemType Directory -Path $packageDir | Out-Null
    tar -xzf $archive -C $packageDir
    if ($LASTEXITCODE -ne 0) { throw 'Failed to extract Syndrid package.' }

    $source = Join-Path $packageDir 'bin\syndrid.exe'
    $metadataPath = Join-Path $packageDir 'codex-package.json'
    if (-not (Test-Path $source -PathType Leaf)) { throw 'Package does not contain bin\syndrid.exe.' }
    if (-not (Test-Path $metadataPath -PathType Leaf)) { throw 'Package does not contain codex-package.json.' }

    New-Item -ItemType Directory -Force -Path $releasesDir | Out-Null
    $releaseDir = Join-Path $releasesDir "$tag-$target"
    Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $releaseDir
    Move-Item $packageDir $releaseDir

    Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $currentDir
    Copy-Item -Recurse -Force $releaseDir $currentDir

    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    $launcher = Join-Path $InstallDir 'syndrid.cmd'
    $entrypoint = Join-Path $currentDir 'bin\syndrid.exe'
    Set-Content -Encoding Ascii -Path $launcher -Value "@echo off`r`n`"$entrypoint`" %*`r`n"
    Write-Host "Installed syndrid from $tag to $releaseDir"
    Write-Host "Entrypoint: $launcher"
}
finally {
    Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $tmp
}
