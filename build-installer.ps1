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


# Create the installer
Write-Host "Creating installer..." -ForegroundColor Yellow
$version = (Select-String -Path Cargo.toml -Pattern '^version').Line -replace '.*"(.+)".*','$1'
& makensis "-DPRODUCT_VERSION=$version" installer.nsi

if ($LASTEXITCODE -eq 0) {
    Write-Host "Installer created successfully: windmenu-installer.exe" -ForegroundColor Green
} else {
    Write-Host "Error: installer failed to build" -ForegroundColor Red
    Read-Host "Press Enter to exit"
    exit 1 
}

Write-Host ""
Write-Host "Build completed successfully!" -ForegroundColor Green
Write-Host ""
Read-Host "Press Enter to exit"
