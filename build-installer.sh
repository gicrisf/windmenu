#!/bin/bash

# Build Windmenu Installer for Linux/WSL
echo -e "\033[0;32mBuilding Windmenu Installer...\033[0m"
echo

# Check if NSIS is installed
if ! command -v makensis &> /dev/null; then
    echo -e "\033[0;31mError: NSIS not found in PATH. Please install NSIS and add it to your PATH.\033[0m"
    echo -e "\033[0;33mInstall with: sudo apt install nsis\033[0m"
    exit 1
fi

# Build the Rust projects first
echo -e "\033[0;33mBuilding Rust projects...\033[0m"
RUSTFLAGS="-C target-feature=+crt-static" \
    cargo build --release --target x86_64-pc-windows-gnu --target-dir ./target

# Help NSIS
ln -sf "$(pwd)/target/x86_64-pc-windows-gnu/release/windmenu.exe" "target/release/"
ln -sf "$(pwd)/target/x86_64-pc-windows-gnu/release/windmenu-monitor.exe" "target/release/"

if [ $? -ne 0 ]; then
    echo -e "\033[0;31mError: Failed to build Rust projects\033[0m"
    exit 1
fi

# Check if required files exist
if [ ! -f "target/release/windmenu.exe" ]; then
    echo -e "\033[0;31mError: windmenu.exe not found in target/release/\033[0m"
    echo -e "\033[0;33mPlease build the project first with: cargo build --release\033[0m"
    exit 1
fi

if [ ! -f "target/release/windmenu-monitor.exe" ]; then
    echo -e "\033[0;31mError: windmenu-monitor.exe not found in target/release/\033[0m"
    echo -e "\033[0;33mPlease build the project first with: cargo build --release\033[0m"
    exit 1
fi

if [ ! -f "assets/wlines-daemon.exe" ]; then
    echo -e "\033[0;33mwlines-daemon.exe not found in assets/, downloading...\033[0m"
    
    # Create assets directory if it doesn't exist
    if [ ! -d "assets" ]; then
        mkdir -p assets
        echo -e "\033[0;32mCreated assets/ directory\033[0m"
    fi
    
    url="https://github.com/gicrisf/wlines/releases/download/v0.1.0/wlines-daemon.exe"
    output="assets/wlines-daemon.exe"
    
    echo -e "\033[0;36mDownloading from: $url\033[0m"
    
    if command -v wget &> /dev/null; then
        wget -O "$output" "$url"
    elif command -v curl &> /dev/null; then
        curl -L -o "$output" "$url"
    else
        echo -e "\033[0;31mError: Neither wget nor curl found. Please install one of them.\033[0m"
        exit 1
    fi
    
    if [ -f "$output" ]; then
        echo -e "\033[0;32mSuccessfully downloaded wlines-daemon.exe\033[0m"
    else
        echo -e "\033[0;31mError: Failed to download wlines-daemon.exe\033[0m"
        echo -e "\033[0;33mPlease download it manually from: $url\033[0m"
        echo -e "\033[0;33mand place it in the assets/ directory\033[0m"
        exit 1
    fi
fi

# Check for Visual C++ Redistributable
if [ ! -f "assets/vc_redist.x64.exe" ]; then
    echo -e "\033[0;33mvc_redist.x64.exe not found in assets/, downloading...\033[0m"
    
    # Create assets directory if it doesn't exist
    if [ ! -d "assets" ]; then
        mkdir -p assets
        echo -e "\033[0;32mCreated assets/ directory\033[0m"
    fi
    
    vcRedistUrl="https://aka.ms/vs/17/release/vc_redist.x64.exe"
    vcRedistPath="assets/vc_redist.x64.exe"
    
    echo -e "\033[0;36mDownloading Visual C++ Redistributable 2015-2022 (x64)...\033[0m"
    echo -e "\033[0;36mFrom: $vcRedistUrl\033[0m"
    
    if command -v wget &> /dev/null; then
        wget -O "$vcRedistPath" "$vcRedistUrl"
    elif command -v curl &> /dev/null; then
        curl -L -o "$vcRedistPath" "$vcRedistUrl"
    else
        echo -e "\033[0;31mError: Neither wget nor curl found. Please install one of them.\033[0m"
        exit 1
    fi
    
    if [ -f "$vcRedistPath" ]; then
        echo -e "\033[0;32mSuccessfully downloaded vc_redist.x64.exe\033[0m"
        fileSize=$(du -m "$vcRedistPath" | cut -f1)
        echo -e "\033[0;36mFile size: ${fileSize} MB\033[0m"
    else
        echo -e "\033[0;31mError: Failed to download VC++ Redistributable\033[0m"
        echo -e "\033[0;33mPlease download it manually from: $vcRedistUrl\033[0m"
        echo -e "\033[0;33mand save it as: assets/vc_redist.x64.exe\033[0m"
        exit 1
    fi
fi

# Create the installer
echo -e "\033[0;33mCreating installer...\033[0m"
makensis installer.nsi

if [ $? -eq 0 ]; then
    echo -e "\033[0;32mInstaller created successfully: windmenu-installer.exe\033[0m"
else
    echo -e "\033[0;31mError: installer failed to build\033[0m"
    exit 1
fi

echo
echo -e "\033[0;32mBuild completed successfully!\033[0m"
echo
