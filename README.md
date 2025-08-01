# windmenu
Fast like the wind, a WINdows DMENU-like launcher

https://github.com/user-attachments/assets/6e35eaa7-521a-4ec0-946a-990ad032c22f

## Features

- Fast application launcher via hotkey (Win+Space is default)
- Vim-like navigation (hjkl) alongside arrow keys
- Scans Windows Start Menu shortcuts automatically
- Supports custom commands via configuration
- Key combination commands - trigger keyboard shortcuts from the menu
- Background daemon with named pipe communication
- Configurable appearance and behavior (thanks to JerwuQu's work on the original version of [wlines](https://github.com/gicrisf/wlines))

## Daemon Monitor

WindMenu includes a GUI monitor to check the status of both the WindMenu daemon and the WLines daemon:

### Features
- Real-time status monitoring of both daemons
- Clean, native Windows GUI
- Displays Process IDs (PIDs) when daemons are running
- Lists all PIDs when multiple instances are detected (we don't want that)
- Interactive buttons to start/restart/kill processes

## Build

Build all components:

```bash
cargo build --release
```

Build specific components:

```bash
# Main windmenu daemon
cargo build --release --bin windmenu

# Status monitor GUI
cargo build --release --bin windmenu-monitor
```

### Usage

Ensure both daemons are running and press `WIN+SPACE` to start the launcher.

You can check the daemons' status using windmenu-monitor. If you find an inactive daemon, you can start it from windmenu-monitor itself.

After compiling, just:

```bash
.\target\release\windmenu-monitor.exe
```

Example display:
- `● Windmenu: Active (PID: 12345)` - Single instance
- `● Windmenu: Active (2 instances: 12345, 67890)` - Multiple instances
- `● Windmenu: Inactive` - No processes running

### Interactive Controls

The monitor includes clickable buttons for process management.

- Buttons are automatically enabled/disabled based on the current process state.
- Buttons darken and shift slightly when pressed
- Inner shadow effect during press
- Immediate visual response for better user experience

The monitor provides a visual confirmation that both windmenu and wlines daemons are running properly and lets you stop/start them easily.

## Installer

This directory contains the NSIS installer script and build tools for creating a Windmenu installer package.

### Prerequisites

1. NSIS (Nullsoft Scriptable Install System)
2. Rust toolchain

### Building the Installer

The build process is fully automated and handles all dependencies:

**On Windows (PowerShell):**
```powershell
.\build-installer.ps1
```

**On Linux/WSL (Bash):**
```bash
./build-installer.sh
```

Both scripts will:
1. **Build Rust projects**
2. **Download dependencies automatically** if missing:
   - `wlines-daemon.exe` from [wlines releases](https://github.com/gicrisf/wlines/releases/)
3. **Create the NSIS installer** with all components bundled

### Bundled Dependencies

The installer is completely self-contained and includes:
- `windmenu.exe` - Main windmenu daemon
- `windmenu-monitor.exe` - GUI monitor application
- `wlines-daemon.exe` - External wlines daemon dependency

### Installation Options

The installer provides several installation components:

#### Core Installation
1. **Core Files (required)**: Main binaries, configuration, and dependencies
2. **Start Menu Shortcuts**: Creates shortcuts in the Start Menu, including:
   - Windmenu Monitor (primary interface for status checking and daemon management)
   - Uninstall
3. **Desktop Shortcut**: Creates a desktop shortcut for Windmenu Monitor
4. **Auto-startup Options**

#### Auto-startup Options
Choose **one** of the following startup methods:

1. Registry Run (Suggested)
2. Task Scheduler
3. Current User Startup Folder
4. All Users Startup Folder (affects all users and requires permissions)

### Dependency Management

#### Runtime Dependencies
WindMenu uses static linking to minimize external dependencies. No additional runtime libraries are required on modern Windows 10/11 systems. (Should theoretically work on any Windows version from XP onward, though it has only been tested on Windows 10/11)

### Default Installation Location

The installer defaults to installing in `%LOCALAPPDATA%\windmenu`, but users can choose a different location during installation.

### Files Created

After installation, the following structure will be created:

```
%LOCALAPPDATA%\windmenu\
├── windmenu.exe
├── windmenu-monitor.exe
├── wlines-daemon.exe
└── uninstall.exe
```

### Uninstallation

The installer creates an uninstaller that:
- Removes all installed files and created directories
- Cleans up all startup methods (Registry, Task Scheduler, Startup folders)
- Removes all shortcuts and registry entries
- Can be accessed through:
  - Control Panel → Programs and Features
  - Start Menu → Windmenu → Uninstall
  - Directly running `uninstall.exe` from the installation directory

The uninstaller ensures complete removal regardless of which startup method was selected during installation.

For manual verification, check the installation directory (typically `%LOCALAPPDATA%\windmenu\` or your custom location). The application is portable, so all binaries and configurations reside within this directory. Simply deleting it will remove all traces from your system, as no other files are stored elsewhere. 

> N.B. The uninstaller removes only installed files, keeping any manually edited configs. These remain for future reinstalls. Delete them manually if needed.

## Key Combination Commands

WindMenu supports executing key combinations as commands! 

Add them to your `windmenu.toml`:

```toml
[[commands]]
name = "alt+x key combo"
keys = ["ALT", "X"]

[[commands]]
name = "win+d show desktop"
keys = ["WIN", "D"]

[[commands]]
name = "switch virtual desktop"
keys = ["WIN", "CTRL", "RIGHT"]
```

### Supported Keys

#### Modifier Keys
- `ALT` - Alt key
- `CTRL` or `CONTROL` - Control key  
- `SHIFT` - Shift key
- `WIN` or `WINDOWS` - Windows key

#### Special Keys
- `TAB` - Tab key
- `ESC` or `ESCAPE` - Escape key
- `SPACE` - Space bar
- `ENTER` - Enter/Return key

#### Function Keys
- `F1`, `F2`, `F3`, ..., `F12` - Function keys

#### Arrow Keys
- `LEFT`, `UP`, `RIGHT`, `DOWN` - Arrow keys

#### Number Keys
- `0`, `1`, `2`, ..., `9` - Number keys

#### Punctuation Keys
- `COMMA` or `,` - Comma key
- `PERIOD` or `.` - Period key
- `SEMICOLON` or `;` - Semicolon key
- `SLASH` or `/` - Forward slash key
- `BACKSLASH` or `\` - Backslash key
- `QUOTE` or `'` - Single quote key
- `BACKTICK` or `` ` `` - Backtick key
- `MINUS` or `-` - Minus/hyphen key
- `EQUALS` or `=` - Equals key
- `LBRACKET` or `[` - Left bracket key
- `RBRACKET` or `]` - Right bracket key

#### Letter Keys
- `A`, `B`, `C`, ..., `Z` - Letter keys (case insensitive)

### How It Works

When you select a key combination command from the menu, WindMenu will:

1. Press all the specified keys down in order
2. Release all the keys in reverse order
3. This simulates the key combination being pressed

### Mixed Configuration

You can mix regular commands and key combinations in the same configuration:

```toml
# Regular command
[[commands]]
name = "open terminal"
args = ["wt"]

# Key combination command
[[commands]]
name = "switch to desktop 1"
keys = ["WIN", "CTRL", "1"]
```

The menu will display both types of commands and execute them appropriately based on their configuration.
