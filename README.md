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

## Installation

### Option 1: PowerShell Script

```powershell
iex "& {$(irm https://raw.githubusercontent.com/gicrisf/windmenu/main/install.ps1)}"
```

This downloads the latest release to `$HOME\.windmenu`, optionally adds it to your PATH, and prints next steps. No admin required.

> **Note**: Windows Defender may flag `iex` (Invoke-Expression) as suspicious. If that happens, you can download and review `install.ps1` manually before running it.

### Option 2: Scoop

```powershell
scoop bucket add gicrisf https://github.com/gicrisf/bucket
scoop install windmenu
```

Then start the daemons: `windmenu daemon all start`

### Option 3: Direct Download

Download `windmenu-portable.zip` from the [latest release](https://github.com/gicrisf/windmenu/releases/latest), extract it, and run `.\windmenu.exe daemon all start`.

Press `Win+Space` to launch.

### Auto-Startup

To have windmenu start automatically when you log in:

```powershell
windmenu daemon all enable task
```

For Scoop installs, use `task` or `user-folder` methods to avoid a brief terminal flash caused by the Scoop shim. See all available methods with `windmenu daemon all enable --help`.

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

## Uninstallation

First, stop the daemons and remove auto-startup entries, otherwise the system will try to launch something that no longer exists at the next startup:

```powershell
windmenu daemon all stop
windmenu daemon all disable
```

If installed via Scoop:

```powershell
scoop uninstall windmenu
```

For script or manual installs, delete the install directory. The application is fully portable (all binaries and configuration reside within it, so no traces are left elsewhere).

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

## Troubleshooting

### Auto-startup with Task Scheduler requires elevated privileges

The `task` method uses `schtasks.exe`, which may fail if your account lacks permission to create scheduled tasks. If you get an access denied error, try `user-folder` instead:

```powershell
windmenu daemon all enable user-folder
```

This places a VBS wrapper in your Startup folder and requires no special privileges.

### Brief terminal flash on startup with Scoop

If you installed via Scoop and enabled auto-startup with the `registry` method, you may see a console window flash briefly when Windows launches the Scoop shim. Switch to `task` or `user-folder` to avoid this:

```powershell
windmenu daemon all disable
windmenu daemon all enable user-folder
```

## Acknowledgments

WindMenu wouldn't be possible without the contributions of others:

- **[wlines](https://github.com/gicrisf/wlines)** - The excellent menu rendering engine that powers WindMenu's interface. Special thanks to [JerwuQu](https://github.com/JerwuQu/wlines) for the original implementation.
- **[dmenu](https://tools.suckless.org/dmenu/)** - The original inspiration for this project. WindMenu aims to bring dmenu's philosophy and efficiency to Windows.
- **[winapi-rs](https://github.com/retep998/winapi-rs)** maintainers - For providing comprehensive Rust bindings to the Windows API, making native Windows development in Rust possible.
