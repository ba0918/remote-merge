# remote-merge installer for Windows
# Usage: irm https://raw.githubusercontent.com/ba0918/remote-merge/main/scripts/install.ps1 | iex
#
# Environment variables:
#   $env:VERSION     - specific version to install (default: latest)
#   $env:INSTALL_DIR - install directory (default: $HOME\.local\bin)

$ErrorActionPreference = "Stop"

$Repo = "ba0918/remote-merge"
$Binary = "remote-merge"
$Target = "x86_64-pc-windows-msvc"

# --- Helper functions ---

function Write-Info($msg) {
    Write-Host "==> " -ForegroundColor Blue -NoNewline
    Write-Host $msg
}

function Write-Err($msg) {
    Write-Host "error: " -ForegroundColor Red -NoNewline
    Write-Host $msg
    exit 1
}

# --- Resolve install directory ---

$InstallDir = if ($env:INSTALL_DIR) {
    $env:INSTALL_DIR
} else {
    Join-Path $HOME ".local\bin"
}

# --- Resolve version ---

function Resolve-Version {
    if ($env:VERSION) {
        return $env:VERSION
    }

    Write-Info "fetching latest release..."
    try {
        $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest"
        return $release.tag_name
    } catch {
        Write-Err "failed to fetch latest release: $_"
    }
}

# --- Main ---

$Version = Resolve-Version
$Archive = "$Binary-$Target.zip"
$BaseUrl = "https://github.com/$Repo/releases/download/$Version"

Write-Info "version: $Version"
Write-Info "target: $Target"

$TmpDir = Join-Path ([System.IO.Path]::GetTempPath()) ([System.Guid]::NewGuid().ToString())
New-Item -ItemType Directory -Path $TmpDir -Force | Out-Null

try {
    # Download archive
    Write-Info "downloading $Archive..."
    $ArchivePath = Join-Path $TmpDir $Archive
    Invoke-WebRequest -Uri "$BaseUrl/$Archive" -OutFile $ArchivePath

    # Download and verify checksum
    Write-Info "verifying checksum..."
    $ChecksumPath = Join-Path $TmpDir "SHA256SUMS.txt"
    Invoke-WebRequest -Uri "$BaseUrl/SHA256SUMS.txt" -OutFile $ChecksumPath

    $expectedLine = Get-Content $ChecksumPath | Where-Object { $_ -match $Archive }
    if (-not $expectedLine) {
        Write-Err "checksum not found for $Archive in SHA256SUMS.txt"
    }
    $expectedHash = ($expectedLine -split '\s+')[0]

    $actualHash = (Get-FileHash -Path $ArchivePath -Algorithm SHA256).Hash.ToLower()

    if ($expectedHash -ne $actualHash) {
        Write-Err "checksum mismatch!`n  expected: $expectedHash`n  actual:   $actualHash"
    }
    Write-Info "checksum OK"

    # Extract
    Write-Info "extracting..."
    Expand-Archive -Path $ArchivePath -DestinationPath $TmpDir -Force

    # Install
    if (-not (Test-Path $InstallDir)) {
        New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    }

    $src = Join-Path $TmpDir "$Binary.exe"
    $dst = Join-Path $InstallDir "$Binary.exe"
    Move-Item -Path $src -Destination $dst -Force

    Write-Info "installed $Binary $Version to $dst"

    # Check if install dir is in PATH
    $inPath = $env:PATH -split ';' | Where-Object { $_ -eq $InstallDir }
    if (-not $inPath) {
        Write-Host ""
        Write-Host "NOTE: " -ForegroundColor Yellow -NoNewline
        Write-Host "$InstallDir is not in your PATH."
        Write-Host "Add it by running:"
        Write-Host ""
        Write-Host "  `$env:PATH += `";$InstallDir`"" -ForegroundColor Cyan
        Write-Host ""
        Write-Host "Or permanently (current user):"
        Write-Host ""
        Write-Host "  [Environment]::SetEnvironmentVariable('PATH', `$env:PATH + ';$InstallDir', 'User')" -ForegroundColor Cyan
        Write-Host ""
    }

    Write-Host ""
    Write-Host "Installation complete!" -ForegroundColor Green
    Write-Host "Run '$Binary --help' to get started."

} finally {
    Remove-Item -Path $TmpDir -Recurse -Force -ErrorAction SilentlyContinue
}
