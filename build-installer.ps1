# Build Windmenu Installer
Write-Host "Building Windmenu Installer..." -ForegroundColor Green
Write-Host ""

# Check if NSIS is installed
$nsisPath = Get-Command makensis -ErrorAction SilentlyContinue
if (-not $nsisPath) {
    Write-Host "Error: NSIS not found in PATH. Please install NSIS and add it to your PATH." -ForegroundColor Red
    Write-Host "Download from: https://nsis.sourceforge.io/" -ForegroundColor Yellow
    Read-Host "Press Enter to exit"
    exit 1
}

# Build the Rust projects first
Write-Host "Building Rust projects..." -ForegroundColor Yellow
cargo build --release
if ($LASTEXITCODE -ne 0) {
    Write-Host "Error: Failed to build Rust projects" -ForegroundColor Red
    Read-Host "Press Enter to exit"
    exit 1
}

# Check if required files exist
if (-not (Test-Path "target\release\windmenu.exe")) {
    Write-Host "Error: windmenu.exe not found in target\release\" -ForegroundColor Red
    Write-Host "Please build the project first with: cargo build --release" -ForegroundColor Yellow
    Read-Host "Press Enter to exit"
    exit 1
}

if (-not (Test-Path "target\release\windmenu-monitor.exe")) {
    Write-Host "Error: windmenu-monitor.exe not found in target\release\" -ForegroundColor Red
    Write-Host "Please build the project first with: cargo build --release" -ForegroundColor Yellow
    Read-Host "Press Enter to exit"
    exit 1
}

if (-not (Test-Path "assets\wlines-daemon.exe")) {
    Write-Host "wlines-daemon.exe not found in assets\, downloading..." -ForegroundColor Yellow
    
    # Create assets directory if it doesn't exist
    if (-not (Test-Path "assets")) {
        New-Item -ItemType Directory -Path "assets" | Out-Null
        Write-Host "Created assets\ directory" -ForegroundColor Green
    }
    
    try {
        $url = "https://github.com/gicrisf/wlines/releases/download/v0.0.1/wlines-daemon.exe"
        $output = "assets\wlines-daemon.exe"
        
        Write-Host "Downloading from: $url" -ForegroundColor Cyan
        Invoke-WebRequest -Uri $url -OutFile $output -UseBasicParsing
        
        if (Test-Path $output) {
            Write-Host "Successfully downloaded wlines-daemon.exe" -ForegroundColor Green
        } else {
            throw "Download completed but file not found"
        }
    } catch {
        Write-Host "Error: Failed to download wlines-daemon.exe" -ForegroundColor Red
        Write-Host "Please download it manually from: $url" -ForegroundColor Yellow
        Write-Host "and place it in the assets\ directory" -ForegroundColor Yellow
        Read-Host "Press Enter to exit"
        exit 1
    }
}

# Create the installer
Write-Host "Creating installer..." -ForegroundColor Yellow

# Try the full installer first, fall back to simple version if there are plugin issues
Write-Host "Attempting to build full installer (with PATH support)..." -ForegroundColor Cyan
& makensis installer.nsi

if ($LASTEXITCODE -eq 0) {
    Write-Host "Full installer created successfully: windmenu-installer.exe" -ForegroundColor Green
} else {
    Write-Host "Full installer failed (likely due to missing NSIS plugins), creating simple version..." -ForegroundColor Yellow
    & makensis installer-simple.nsi
    
    if ($LASTEXITCODE -eq 0) {
        Write-Host "Simple installer created successfully: windmenu-installer-simple.exe" -ForegroundColor Green
        Write-Host "Note: This version doesn't modify PATH automatically" -ForegroundColor Yellow
    } else {
        Write-Host "Error: Both installers failed to build" -ForegroundColor Red
        Read-Host "Press Enter to exit"
        exit 1
    }
}

Write-Host ""
Write-Host "Build completed successfully!" -ForegroundColor Green
Write-Host ""
Read-Host "Press Enter to exit"
