$ErrorActionPreference = 'Stop'

$Repo = 'puppycorp/puppylog'
$BinName = 'plog'

if (-not $env:INSTALL_DIR -or [string]::IsNullOrWhiteSpace($env:INSTALL_DIR)) {
    $InstallDir = Join-Path $env:LOCALAPPDATA 'Programs\plog\bin'
} else {
    $InstallDir = $env:INSTALL_DIR
}

if (-not $env:VERSION -or [string]::IsNullOrWhiteSpace($env:VERSION)) {
    $Version = 'latest'
} else {
    $Version = $env:VERSION
}

function Get-TargetTriple {
    $arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture
    switch ($arch) {
        'X64' { return 'x86_64-pc-windows-msvc' }
        default { throw "Unsupported Windows architecture: $arch" }
    }
}

function Resolve-Version {
    param([string]$RequestedVersion)

    if ($RequestedVersion -ne 'latest') {
        return $RequestedVersion
    }

    $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest"
    if (-not $release.tag_name) {
        throw 'Failed to resolve latest release tag'
    }
    return $release.tag_name
}

function Verify-Checksum {
    param(
        [string]$ArchivePath,
        [string]$ChecksumPath
    )

    $archiveName = Split-Path $ArchivePath -Leaf
    $expected = (Get-Content $ChecksumPath -Raw).Replace("dist/$archiveName", $archiveName).Trim().Split()[0].ToLowerInvariant()
    $actual = (Get-FileHash -Path $ArchivePath -Algorithm SHA256).Hash.ToLowerInvariant()

    if ($expected -ne $actual) {
        throw "Checksum verification failed for $archiveName"
    }
}

function Ensure-PathContainsInstallDir {
    param([string]$Dir)

    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    $parts = @()
    if ($userPath) {
        $parts = $userPath.Split(';', [System.StringSplitOptions]::RemoveEmptyEntries)
    }

    if ($parts -contains $Dir) {
        return
    }

    $newPath = if ([string]::IsNullOrWhiteSpace($userPath)) {
        $Dir
    } else {
        "$userPath;$Dir"
    }

    [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
    Write-Host "Added $Dir to your user PATH. Restart your terminal to pick it up."
}

$target = Get-TargetTriple
$resolvedVersion = Resolve-Version -RequestedVersion $Version
$archiveName = "$BinName-$resolvedVersion-$target.zip"
$assetUrl = "https://github.com/$Repo/releases/download/$resolvedVersion/$archiveName"
$checksumUrl = "$assetUrl.sha256"
$tmpDir = Join-Path ([System.IO.Path]::GetTempPath()) ([System.Guid]::NewGuid().ToString())

try {
    New-Item -ItemType Directory -Path $tmpDir | Out-Null

    $archivePath = Join-Path $tmpDir $archiveName
    $checksumPath = "$archivePath.sha256"
    $extractDir = Join-Path $tmpDir 'extract'
    $binaryPath = Join-Path $extractDir "$BinName.exe"

    Write-Host "Installing $BinName $resolvedVersion for $target..."
    Invoke-WebRequest -Uri $assetUrl -OutFile $archivePath
    Invoke-WebRequest -Uri $checksumUrl -OutFile $checksumPath

    Verify-Checksum -ArchivePath $archivePath -ChecksumPath $checksumPath

    Expand-Archive -Path $archivePath -DestinationPath $extractDir -Force

    if (-not (Test-Path $binaryPath)) {
        throw "Archive did not contain $BinName.exe"
    }

    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    Copy-Item -Path $binaryPath -Destination (Join-Path $InstallDir "$BinName.exe") -Force

    Write-Host "Installed to $(Join-Path $InstallDir "$BinName.exe")"
    Ensure-PathContainsInstallDir -Dir $InstallDir
} finally {
    if (Test-Path $tmpDir) {
        Remove-Item -Path $tmpDir -Recurse -Force
    }
}
