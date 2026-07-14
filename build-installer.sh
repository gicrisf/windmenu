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
mkdir -p target/release
ln -sf "$(pwd)/target/x86_64-pc-windows-gnu/release/windmenu.exe" "target/release/"

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
