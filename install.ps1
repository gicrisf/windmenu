# windmenu installer
# Usage: iex "& {$(irm https://raw.githubusercontent.com/gicrisf/windmenu/main/install.ps1)}"

$ErrorActionPreference = "Stop"
$InstallDir = "$HOME\.windmenu"

Write-Host ""
Write-Host "windmenu installer" -ForegroundColor Cyan
Write-Host "==================" -ForegroundColor Cyan
Write-Host ""

# Detect latest version from GitHub API
Write-Host "Fetching latest release..." -ForegroundColor Yellow
try {
    $release = Invoke-RestMethod -Uri "https://api.github.com/repos/gicrisf/windmenu/releases/latest" -UseBasicParsing
    $tag = $release.tag_name
} catch {
    Write-Host "Error: Failed to fetch latest release from GitHub." -ForegroundColor Red
    Write-Host $_.Exception.Message -ForegroundColor Red
    exit 1
}

Write-Host "Latest version: $tag" -ForegroundColor Green
Write-Host "Install directory: $InstallDir" -ForegroundColor Green
Write-Host ""

# Download portable zip to temp
$zipUrl = "https://github.com/gicrisf/windmenu/releases/download/$tag/windmenu-portable.zip"
$tempZip = Join-Path $env:TEMP "windmenu-portable.zip"

Write-Host "Downloading $zipUrl ..." -ForegroundColor Yellow
try {
    Invoke-WebRequest -Uri $zipUrl -OutFile $tempZip -UseBasicParsing
} catch {
    Write-Host "Error: Failed to download windmenu-portable.zip" -ForegroundColor Red
    Write-Host $_.Exception.Message -ForegroundColor Red
    exit 1
}

# Create install directory and extract
if (-not (Test-Path $InstallDir)) {
    New-Item -ItemType Directory -Path $InstallDir | Out-Null
}

Write-Host "Extracting to $InstallDir ..." -ForegroundColor Yellow
try {
    Expand-Archive -Path $tempZip -DestinationPath $InstallDir -Force
} catch {
    Write-Host "Error: Failed to extract archive." -ForegroundColor Red
    Write-Host $_.Exception.Message -ForegroundColor Red
    exit 1
}

Remove-Item $tempZip -ErrorAction SilentlyContinue

# Verify extraction
$windmenuExe = Join-Path $InstallDir "windmenu.exe"
if (-not (Test-Path $windmenuExe)) {
    Write-Host "Error: windmenu.exe not found after extraction." -ForegroundColor Red
    exit 1
}

# Ask user about PATH
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
$alreadyInPath = $userPath -split ';' | Where-Object { $_ -eq $InstallDir }

if ($alreadyInPath) {
    Write-Host "$InstallDir is already in your PATH." -ForegroundColor Green
} else {
    Write-Host ""
    $addPath = Read-Host "Add $InstallDir to your user PATH? (Y/n)"
    if ($addPath -eq '' -or $addPath -match '^[Yy]') {
        $newPath = "$userPath;$InstallDir"
        [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
        $env:Path = "$env:Path;$InstallDir"
        Write-Host "Added to PATH." -ForegroundColor Green
    } else {
        Write-Host "Skipped. You can run windmenu from $InstallDir directly." -ForegroundColor Yellow
    }
}

# Done
Write-Host ""
Write-Host "windmenu $tag installed to $InstallDir" -ForegroundColor Green
Write-Host ""
Write-Host "To get started, run:" -ForegroundColor Cyan
Write-Host "  windmenu fetch wlines-daemon" -ForegroundColor White
Write-Host "  windmenu daemon all start" -ForegroundColor White
Write-Host ""
Write-Host "Then press Win+Space to launch the menu." -ForegroundColor Cyan
Write-Host ""
Write-Host "Optional:" -ForegroundColor Cyan
Write-Host "  windmenu daemon all enable task    # auto-start on login" -ForegroundColor White
Write-Host "  notepad $InstallDir\windmenu.toml  # customize hotkey and commands" -ForegroundColor White
Write-Host ""
