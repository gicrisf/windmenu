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
- Configurable appearance and behavior

## Installation

### Option 1: PowerShell

```powershell
iex "& {$(irm https://raw.githubusercontent.com/gicrisf/windmenu/main/install.ps1)}"
```

No admin required.

Press `Ctrl+Alt+Space` to launch.

> Windows Defender may flag `iex` as suspicious. If so, check out the options below.

### Option 2: ZIP

The previous script downloads the latest release to `$HOME\.windmenu`, optionally adds it to your PATH, and lets you select an autostart method.

You can do the exact same thing manually:
- Download [`windmenu.zip`](https://github.com/gicrisf/windmenu/releases/latest)
- Extract it wherever you prefer (e.g. `$HOME\.windmenu`)
- Run `.\windmenu.exe start`
- To start at boot, see [`autostart/README.md`](autostart/README.md).

### Option 3: Scoop

`scoop bucket add gicrisf https://github.com/gicrisf/bucket && scoop install windmenu`

### Option 4: Cargo

`cargo install --git https://github.com/gicrisf/windmenu`

Compiles the current development branch; requires a Rust toolchain on your Windows host.

## Configuration

### Interaction

Press `Ctrl+Alt+Space` to open the menu. Navigate with arrow keys or
`Ctrl+J`/`Ctrl+K` (vim-style). Both the activation hotkey and the navigation
keys are configurable in `windmenu.toml`.

### Window

The menu comes in two layouts: **vertical** (default) and **horizontal**.

By default, the menu is a centered rectangle with a vertical list of entries.
The `lines` setting controls how many items are visible at once (default: 12):

```toml
lines  = 12      # menu items shown at once (vertical only)
width  = 1000    # window width in pixels
center = true    # center on the monitor under the cursor
```

For a dmenu `-h` style single-row bar at the top of the screen, switch to
horizontal layout:

```toml
horizontal = true
width      = 0       # 0 = full monitor width
center     = false   # pin to top-left
```

In horizontal mode entries flow left-to-right in a single row, the input box
is capped at a quarter of the window width, and `<` / `>` page markers appear
when there are more entries than fit on screen. The `lines` setting is ignored.

### Theming

A theme is just six colors. windmenu ships with a built-in scheme that is active by default; override any of the six keys at the top level of `windmenu.toml` to tweak it, or set all six to define your own:

```toml
bg        = "#1e1e1e"   # Window background
fg        = "#ffffff"   # Window text
bg_select = "#0078d4"   # Selected item background
fg_select = "#ffffff"   # Selected item text
bg_input  = "#2d2d2d"   # Input box background
fg_input  = "#ffffff"   # Input box text
```

To keep several named schemes on hand and switch between them, see [Config packs](#config-packs).

## Menu

Two types of entries appear in the menu:

### Applications

Discovered from your Start Menu automatically. The scan runs in the background
at startup, so the hotkey works immediately.

### Commands

Add your own with `[[commands]]` entries in `windmenu.toml`:

```toml
[[commands]]
name = "Terminal"
args = ["wt"]

[[commands]]
name = "Show Desktop"
keys = ["WIN", "D"]
```

`args` runs a program; `keys` simulates a keyboard shortcut.

A few commands are always available:

- **Toggle Caps Lock** — handy when the physical key is remapped
- **Refresh Apps** — rescan applications without restarting
- **Reload Config** — reload commands from `windmenu.toml`

## Supported Keys

Valid key names for `keys = [...]` command combinations: modifiers (`ALT`, `CTRL`, `SHIFT`, `WIN`), `F1`–`F12`, arrow keys, `A`–`Z`, `0`–`9`, and special keys (`TAB`, `ESC`, `SPACE`, `ENTER`, punctuation). See [KEYS.md](KEYS.md) for the full reference.

## Config packs

Themes and commands can live in standalone pack files that you pull in with
`import`. Ready-made ones live in a separate repo,
[windmenu-packs](https://github.com/gicrisf/windmenu-packs) — clone it next to
your `windmenu.toml` and import what you want:

```toml
import = ["packs/themes/catppuccin.toml", "packs/commands/power.toml"]
theme  = "catppuccin-mocha"
```

A theme is inert until you select it with `theme`; a command pack activates on
import. Imports are non-recursive and missing/broken files are warned-and-skipped,
so windmenu always starts. Your `windmenu.toml` always wins over imports, and
among imports the later one wins.

## Uninstallation

For a portable or scripted installation: stop the daemon (`windmenu stop`),
disable auto-start (`autostart.ps1 disable` — see [autostart](autostart/README.md)),
then delete the installation folder.

Scoop and Cargo users: `scoop uninstall windmenu` or `cargo uninstall windmenu`
handle everything on their own.

## Troubleshooting

If windmenu doesn't pick up your configuration, run `windmenu doctor` to see
which config file is being loaded. If Windows Store apps are missing,
`windmenu test reparse-points` checks that Store-app detection is working.

## Acknowledgments

WindMenu wouldn't be possible without the contributions of others:

- **[wlines](https://github.com/gicrisf/wlines)** - The excellent menu rendering engine that WindMenu's built-in renderer is ported from. Special thanks to [JerwuQu](https://github.com/JerwuQu/wlines) for the original implementation.
- **[dmenu](https://tools.suckless.org/dmenu/)** - The original inspiration for this project. WindMenu aims to bring dmenu's philosophy and efficiency to Windows.
- **[winapi-rs](https://github.com/retep998/winapi-rs)** maintainers - For providing comprehensive Rust bindings to the Windows API, making native Windows development in Rust possible.
