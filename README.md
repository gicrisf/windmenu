# windmenu
Fast like the wind, a WINdows DMENU-like launcher

https://github.com/user-attachments/assets/6e35eaa7-521a-4ec0-946a-990ad032c22f

## Features

- Fast application launcher via hotkey (Ctrl+Alt+Space is default)
- Vim-like navigation (Ctrl+J/K) alongside arrow keys
- Scans Windows Start Menu shortcuts automatically
- Supports custom commands via configuration
- Key combination commands - trigger keyboard shortcuts from the menu
- Single self-contained executable - the menu renderer is built in
- Configurable appearance and behavior (thanks to JerwuQu's work on the original version of [wlines](https://github.com/gicrisf/wlines), which the built-in renderer is ported from)

## Installation

### Option 1: PowerShell Script

```powershell
iex "& {$(irm https://raw.githubusercontent.com/gicrisf/windmenu/main/install.ps1)}"
```

This downloads the latest release to `$HOME\.windmenu`, optionally adds it to your PATH, and prints next steps. No admin required.

> **Note**: Windows Defender may flag `iex` (Invoke-Expression) as suspicious. If that happens, you can download and review `install.ps1` manually before running it.

### Option 2: Scoop

```
scoop bucket add gicrisf https://github.com/gicrisf/bucket
scoop install windmenu
```

### Option 3: Direct Download

Download `windmenu-portable.zip` from the [latest release](https://github.com/gicrisf/windmenu/releases/latest), extract it, and run `.\windmenu.exe daemon start`.

Press `Ctrl+Alt+Space` to launch.

### Option 4: Cargo Install

```bash
cargo install --git https://github.com/gicrisf/windmenu
```

> **Note**: Cargo compiles windmenu from source on your Windows host, so a Rust
> toolchain is required. The precompiled binary from a [release](https://github.com/gicrisf/windmenu/releases/latest)
> is usually the simpler option. `cargo uninstall windmenu` removes the binary
> but does not stop a running daemon nor clean up auto-start entries — run
> `windmenu daemon stop` followed by `windmenu daemon disable` before uninstalling.

Press `Ctrl+Alt+Space` to launch.

### Auto-Startup

To have windmenu start automatically when you log in:

```powershell
windmenu daemon enable user-folder
```

This places a `windmenu.lnk` shortcut in your Startup folder. No admin required. Alternatively, use `registry` to add windmenu to the Run key (`HKCU\...\Run`). See all available methods with `windmenu daemon enable --help`.

### Customizing the Hotkey

To customize the hotkey or add your own commands, edit `windmenu.toml` in the same directory as the executable:

```toml
# Change the hotkey
hotkey = ["CTRL", "ALT", "SPACE"]  # Default

# Other combos to try:
# hotkey = ["WIN", "R"]
# hotkey = ["CTRL", "SPACE"]
# hotkey = ["WIN", "SHIFT", "SPACE"]
```

For full configuration options, check the [example windmenu.toml](https://github.com/gicrisf/windmenu/blob/main/windmenu.toml) in the repository.

## Commands

The menu is populated from three sources:

### 1. Start Menu Shortcuts (discovered automatically)

Windmenu scans for `.lnk` files in the Windows Start Menu directories (`%APPDATA%` and `%ProgramData%`). This is how it finds your installed applications. The scan runs in the background at startup, so the hotkey is available immediately while apps populate over the next few seconds. If you install a new application, use the `Refresh Apps` command from the menu to pick it up without restarting the daemon.

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

These are built-in commands that are always available:

- **Toggle Caps Lock** — useful if you've remapped your physical Caps Lock key but occasionally need to toggle it
- **WLAN Scan** — trigger a WiFi network scan
- **Refresh Apps** — rescan the Start Menu and Windows Store apps without restarting the daemon
- **Reload Config** — reload custom commands from `windmenu.toml` without restarting

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

## Build

Build all components:

```bash
cargo build --release
```

## Uninstallation

First, stop the daemon and remove auto-startup entries, otherwise the system will try to launch something that no longer exists at the next startup:

```powershell
windmenu daemon stop
windmenu daemon disable
```

Check the situation with

``` powershell
windmenu daemon status
```

If no instance is running and no startup configuration is still enabled, proceed by removing the binaries. If installed via Scoop:

```powershell
scoop uninstall windmenu
```

For other installations, delete the installation directory (`$HOME\.windmenu` if you used the script). The application is fully portable (all binaries and configuration reside within it, so no traces are left elsewhere).

## Upgrading from 0.5.x

Since 0.6.0 the menu renderer is built into `windmenu.exe`; the separate `wlines-daemon.exe` process, the named pipe, and the `wlines.exe` fallback are gone. After upgrading:

1. Remove any auto-start entries for the old wlines daemon (with the **old** binary: `windmenu daemon wlines disable <method>`, or delete them manually from `HKCU\...\Run` / Task Scheduler / the Startup folder).
2. Stop and delete any leftover `wlines-daemon.exe`.
3. Delete old `start-windmenu-daemon-*.vbs` / `start-wlines-daemon-*.vbs` files from your Startup folders (`shell:startup`, `shell:common startup`); the `user-folder` method now uses a plain `windmenu.lnk` shortcut.
4. The `wlines_daemon_path` / `wlines_cli_path` config keys and the generated `wlines-config.txt` file are no longer used; the `windmenu fetch` and `windmenu daemon wlines|all` commands were removed (use `windmenu daemon ...`).

## Troubleshooting

### Configuration not being read

If windmenu doesn't seem to pick up your configuration changes, it may be reading `windmenu.toml` from a different location than you expect. Run `windmenu test config` to see which config file is being loaded and what values it contains.

### Windows Store apps not appearing in the menu

Windmenu discovers Windows Store apps by detecting reparse points in the Start Menu directories. If some apps are missing, run `windmenu test reparse-points` to verify that reparse point detection is working correctly on your system.

## Acknowledgments

WindMenu wouldn't be possible without the contributions of others:

- **[wlines](https://github.com/gicrisf/wlines)** - The excellent menu rendering engine that WindMenu's built-in renderer is ported from. Special thanks to [JerwuQu](https://github.com/JerwuQu/wlines) for the original implementation.
- **[dmenu](https://tools.suckless.org/dmenu/)** - The original inspiration for this project. WindMenu aims to bring dmenu's philosophy and efficiency to Windows.
- **[winapi-rs](https://github.com/retep998/winapi-rs)** maintainers - For providing comprehensive Rust bindings to the Windows API, making native Windows development in Rust possible.
