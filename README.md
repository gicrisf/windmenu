# windmenu
Fast like the wind, a WINdows DMENU-like launcher

https://github.com/user-attachments/assets/6e35eaa7-521a-4ec0-946a-990ad032c22f

## Features

- Fast application launcher via hotkey (Ctrl+Alt+Space is default)
- Horizontal single-row bar mode
- Vim-like navigation (Ctrl+J/K) alongside arrow keys
- Scans Windows Start Menu shortcuts automatically
- Supports custom commands via configuration
- Trigger keyboard shortcuts from commands in the menu
- Single self-contained executable
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

Download `windmenu.zip` from the [latest release](https://github.com/gicrisf/windmenu/releases/latest), extract it, and run `.\windmenu.exe start`.

Press `Ctrl+Alt+Space` to launch.

### Option 4: Cargo Install

```bash
cargo install --git https://github.com/gicrisf/windmenu
```

> **Note**: This installs the current development version from the `main` branch
> — it may include unreleased changes. Use the PowerShell script, Scoop, or
> direct download for the latest published release. Cargo compiles windmenu from
> source on your Windows host, so a Rust toolchain is required. `cargo uninstall
> windmenu` removes the binary but does not stop a running daemon nor clean up
> any auto-start shortcut you created — run `windmenu stop` and remove the
> Startup-folder shortcut (see below) before uninstalling.

Press `Ctrl+Alt+Space` to launch.

### Auto-Startup

windmenu no longer manages auto-start itself — it is a plain Startup-folder
shortcut (or registry Run-key entry) that you create once. The Scoop package and
the PowerShell installer set this up for you; for any other install (including
`cargo install`) you enable it by hand.

See [`autostart/README.md`](autostart/README.md) (shipped as `AUTOSTART.md` in
`windmenu.zip`) for the copy-paste commands and the `autostart.ps1` helper.

## Configuration

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

### Window

Set `horizontal = true` for a single-row bar at the top of the screen (dmenu -h style). Entries flow left-to-right with `<` / `>` page markers when they overflow.

```toml
width = 0       # Full screen width
center = false  # Pin to top-left
horizontal = true
```

### Theming

Windmenu ships with a built-in color scheme (a Windows-blue dark look) that is active by default. To tweak it, set any of the six color keys at the top level of `windmenu.toml`:

```toml
bg        = "#1e1e1e"   # Window background
fg        = "#ffffff"   # Window text
bg_select = "#0078d4"   # Selected item background
fg_select = "#ffffff"   # Selected item text
bg_input  = "#2d2d2d"   # Input box background
fg_input  = "#ffffff"   # Input box text
```

If you override them all, you essentially have defined a new theme, since a theme is just these 6 colors hex code. If you need to keep several color schemes on hand, define named themes and switch between them with `theme`:

```toml
theme = "nord"

[themes.nord]
bg        = "#2e3440"
fg        = "#d8dee9"
bg_select = "#5e81ac"
fg_select = "#eceff4"
bg_input  = "#3b4252"
fg_input  = "#d8dee9"
```

`theme = "default"` is reserved for the built-in scheme, so you can always switch back. An unknown theme name is never fatal because windmenu warns (visible in `windmenu config path`) and keeps the built-in colors. Top-level color keys always win over the selected theme, so you can pick a theme and still override just its accent.

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

## Config packs

Themes and commands can live in standalone pack files. See [`packs/README.md`](packs/README.md) for details.

```toml
import = ["packs/catppuccin-theme.toml", "packs/power-commands.toml"]
```

## Build

Build all components:

```bash
cargo build --release
```

## Uninstallation

First, stop the daemon and remove any auto-startup shortcut you created,
otherwise the system will try to launch something that no longer exists at the
next startup:

```powershell
windmenu stop
Remove-Item (Join-Path ([Environment]::GetFolderPath('Startup')) 'windmenu.lnk') -ErrorAction SilentlyContinue
```

Check whether the daemon is still running with

``` powershell
windmenu status
```

If no instance is running, proceed by removing the binaries. If installed via Scoop (which also removes the Startup shortcut automatically):

```powershell
scoop uninstall windmenu
```

For other installations, delete the installation directory (`$HOME\.windmenu` if you used the script). The application is fully portable (all binaries and configuration reside within it, so no traces are left elsewhere).

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
