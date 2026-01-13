# ClipSync Install Script for Windows
# Usage: iwr -useb https://raw.githubusercontent.com/locchung/clipsync/main/install.ps1 | iex

$ErrorActionPreference = "Stop"

$REPO = "locchung/clipsync"
$INSTALL_DIR = "$env:USERPROFILE\.local\bin"
$BINARY_NAME = "clipsync.exe"

Write-Host ""
Write-Host "🔗 ClipSync Installer for Windows" -ForegroundColor Cyan
Write-Host "==================================" -ForegroundColor Cyan
Write-Host ""

# Detect architecture
$ARCH = if ([Environment]::Is64BitOperatingSystem) { "x86_64" } else { "i686" }
Write-Host "[INFO] Detected architecture: $ARCH" -ForegroundColor Green

# Create install directory
if (-not (Test-Path $INSTALL_DIR)) {
    New-Item -ItemType Directory -Path $INSTALL_DIR -Force | Out-Null
    Write-Host "[INFO] Created directory: $INSTALL_DIR" -ForegroundColor Green
}

# Download URL
$DOWNLOAD_URL = "https://github.com/$REPO/releases/latest/download/clipsync-windows-$ARCH.exe"

Write-Host "[INFO] Downloading from: $DOWNLOAD_URL" -ForegroundColor Green

try {
    Invoke-WebRequest -Uri $DOWNLOAD_URL -OutFile "$INSTALL_DIR\$BINARY_NAME" -UseBasicParsing
    Write-Host "[INFO] Downloaded to: $INSTALL_DIR\$BINARY_NAME" -ForegroundColor Green
}
catch {
    Write-Host "[WARN] Pre-built binary not available. Trying cargo install..." -ForegroundColor Yellow
    
    # Check if cargo is available
    if (Get-Command cargo -ErrorAction SilentlyContinue) {
        cargo install --git "https://github.com/$REPO" --bin clipsync
        Write-Host "[INFO] Installed via cargo!" -ForegroundColor Green
        Write-Host ""
        Write-Host "Run 'clipsync' to start syncing clipboards." -ForegroundColor Cyan
        exit 0
    }
    else {
        Write-Host "[ERROR] Cargo not found. Please install Rust: https://rustup.rs" -ForegroundColor Red
        exit 1
    }
}

# Add to PATH if not already there
$currentPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($currentPath -notlike "*$INSTALL_DIR*") {
    [Environment]::SetEnvironmentVariable("Path", "$currentPath;$INSTALL_DIR", "User")
    Write-Host "[INFO] Added $INSTALL_DIR to PATH" -ForegroundColor Green
    Write-Host "[WARN] Please restart your terminal for PATH changes to take effect" -ForegroundColor Yellow
}

Write-Host ""
Write-Host "✅ Installation complete!" -ForegroundColor Green
Write-Host ""
Write-Host "Run 'clipsync' to start syncing clipboards." -ForegroundColor Cyan
Write-Host "(You may need to restart your terminal first)" -ForegroundColor Yellow
Write-Host ""
