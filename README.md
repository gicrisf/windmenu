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


## Quickstart

Download from the [latest release](https://github.com/gicrisf/windmenu/releases/latest):

### Option 1: Installer (GUI Setup)
- Download `windmenu-installer.exe`
- Run the wizard and follow the prompts

Done! Press `Win+Space` and type.

### Option 2: Portable ZIP (CLI Setup)
- Download `windmenu-portable.zip` and extract it
- Setup everything via the windmenu.exe CLI:

```powershell
# Fetch dependencies and start both daemons
.\windmenu.exe fetch wlines-daemon
.\windmenu.exe daemon all start

# Optional: enable auto-startup. Learn how through:
# .\windmenu.exe daemon all enable --help
```

Again, `Win+Space` to launch.

### Customizing the Hotkey

To customize the hotkey or add your own commands, edit `windmenu.toml` in the same directory as the executable:

```toml
# Change the hotkey
shortcut = ["WIN", "SPACE"]  # Default

# If you use multiple keyboard layouts, try one of these instead:
# shortcut = ["WIN", "R"]
# shortcut = ["ALT", "SPACE"]
# shortcut = ["CTRL", "SPACE"]
```

**Note on the default hotkey**: `Win+Space` is Windows' language switcher shortcut. If you only use one keyboard layout (like US English), this won't matter - the conflict is harmless. But if you switch between multiple languages, you'll want to change the hotkey to something else.

For full configuration options, check the [example windmenu.toml](https://github.com/gicrisf/windmenu/blob/main/windmenu.toml) in the repository.

## Commands

The menu is populated from three sources:

### 1. Start Menu Shortcuts (discovered automatically)

Windmenu scans for `.lnk` files in the Windows Start Menu directories (`%APPDATA%` and `%ProgramData%`). This is how it finds your installed applications. The scanning happens once at startup, so the menu appears instantly when you press the hotkey.

### 2. Custom Commands (configured in `windmenu.toml`)

You can add your own commands in two forms:

**PowerShell invocations:**
```toml
[[commands]]
name = "Terminal"
args = ["wt"]
```

**Key combinations:**
```toml
[[commands]]
name = "Show Desktop"
keys = ["WIN", "D"]
```

The key combination support turned out to be more useful than expected. Want to switch virtual desktops? `["WIN", "CTRL", "RIGHT"]`. Toggle between windows? `["ALT", "TAB"]`. These are first-class commands in the menu, no different from launching applications.

Implementation uses Windows `SendInput` API with proper key sequencing (press all keys down in order, release in reverse). There's special handling for toggle keys like Caps Lock, which Windows treats differently.

### 3. Special Commands (always active)

These are built-in commands that are always available, primarily useful for edge cases. For example, the Caps Lock toggle command is handy if you've remapped your physical Caps Lock key to something else but occasionally need to actually toggle caps lock state.

## Build

Build all components:

```bash
cargo build --release
```


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
- `wlines-daemon.exe` - External wlines daemon dependency

### Installation Options

The installer provides several installation components:

#### Core Installation
1. **Core Files (required)**: Main binaries, configuration, and dependencies
2. **Start Menu Shortcuts**: Creates shortcuts in the Start Menu
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

## Acknowledgments

WindMenu wouldn't be possible without the contributions of others:

- **[wlines](https://github.com/gicrisf/wlines)** - The excellent menu rendering engine that powers WindMenu's interface. Special thanks to [JerwuQu](https://github.com/JerwuQu/wlines) for the original implementation.
- **[dmenu](https://tools.suckless.org/dmenu/)** - The original inspiration for this project. WindMenu aims to bring dmenu's philosophy and efficiency to Windows.
- **[winapi-rs](https://github.com/retep998/winapi-rs)** maintainers - For providing comprehensive Rust bindings to the Windows API, making native Windows development in Rust possible.
