## Single-process architecture (0.6.0)

The menu renderer is now a native Rust module inside `windmenu.exe` — a full port of the wlines C renderer. The two-process design (windmenu.exe + wlines-daemon.exe over a named pipe, with wlines.exe CLI fallback) is gone, and with it the main source of instability: pipe races, daemon lifecycle detection, and silent fallback paths.

- One executable, nothing to fetch: `windmenu fetch` and the bundled `wlines-daemon.exe` are removed
- The daemon CLI is flattened: `windmenu daemon <start|stop|restart|status|enable|disable>`. The `self`, `wlines`, and `all` subcommands are gone since there is only one daemon now
- Config keys `wlines_daemon_path` and `wlines_cli_path` are removed and silently ignored; `wlines-config.txt` is no longer generated
- Each hotkey press opens a fresh menu window in-process — no stale daemon state, no orphaned renderer processes
- `filter_mode` values are `complete`, `keywords`, or `fuzzy`. Default is `fuzzy` (fzf-style subsequence scoring, results sorted by score). `"complete"` is now an explicit config value; `"keywords"` is the fallback for unrecognized strings.
- Upgraders: remove old wlines-daemon auto-start entries (see "Upgrading from 0.5.x" in the README)
- Hotkey detection now uses `RegisterHotKey` (event-driven, no idle CPU, no missed or repeated triggers) instead of a 50ms `GetAsyncKeyState` poll loop. Hotkeys must be any number of modifiers (WIN/CTRL/ALT/SHIFT) plus exactly one other key; if registration fails (combo taken by another app), the daemon shows an error dialog and exits instead of silently misbehaving. The `hotkey_poll_interval` config key is removed and silently ignored
- windmenu.exe is now a GUI-subsystem binary: no console window ever flashes, from any launch path (Startup shortcut, registry Run key, Scoop shim). CLI output still works in terminals via console attachment
- The `user-folder` startup method now creates a plain `windmenu.lnk` shortcut instead of a VBS wrapper — VBScript is deprecated by Microsoft, and script-in-Startup-folder artifacts are prime AV/EDR quarantine targets. Existing `.vbs` entries from older versions should be removed manually
- The `all-users-folder` startup method is removed (it required admin; per-user methods cover the actual use cases)

## Flat config with theme presets

- Colors use short keys — `bg`, `fg`, `bg_select`, `fg_select`, `bg_input`, `fg_input`. The built-in theme is always active, so a minimal config sets none of them; set any at the top level to tweak it
- Optional presets for power users: define `[themes.<name>]` blocks and switch with `theme = "<name>"`. `"default"` is reserved for the built-in palette; an unknown name warns (shown in `windmenu config path`) and falls back. Ready-made palettes live in `themes/`
- Run `windmenu config init --force` to regenerate. Old sectioned keys are silently ignored

## Bug Fixes

- Daemon spawning now uses `current_exe` instead of relying on PATH, avoiding the console window flash caused by package manager shims (e.g. Scoop wrappers)
- Executable lookup for `wlines-daemon.exe` and `wlines.exe` uses a shared path resolution helper, checking the directory of the running executable first
- Fixed prompt and font name being rendered with literal quote characters in the wlines config (`-p "run "` &rarr; `-p run `)
- Renamed the TOML config key from `shortcut` to `hotkey` to match the actual field name in the code (the old key was silently ignored)
- Simplified the default `windmenu.toml` (removed excessive comments)
- Readme improvements with new troubleshooting section

## PowerShell install script

New `install.ps1` provides a one-liner install for users without Scoop:

```powershell
iex "& {$(irm https://raw.githubusercontent.com/gicrisf/windmenu/main/install.ps1)}"
```

- Detects latest release from the GitHub API
- Downloads and extracts `windmenu-portable.zip` to `$HOME\.windmenu`
- Optionally adds the install directory to the user PATH
- No admin required

NSIS script is still there, but I stopped suggesting it. I doubt the user of this program could prefer a NSIS installer to command line solutions.

## Cooperative shutdown and AV safety

- Process enumeration (`CreateToolhelp32Snapshot`) and forcible termination (`TerminateProcess`) replaced with cooperative shutdown via named kernel primitives: a named mutex for singleton detection and a named event for graceful stop. No process table walks, no `OpenProcess(PROCESS_TERMINATE)`. The entire `proc.rs` module is removed
- Dropped `tlhelp32` and `processthreadsapi` winapi features; added `synchapi`

## Task Scheduler startup method removed

- The `task` (Task Scheduler) startup method is removed. It depended on `schtasks.exe` and inline XML generation — both flagged by AV behavioral heuristics — and required admin privileges while offering no reliability advantage over the simpler methods
- Only `registry` (Run key) and `user-folder` (plain `.lnk` shortcut) remain. The `task.rs` module is deleted

## Lazy menu loading

- The menu hotkey registers immediately at startup, before the Start Menu scan begins. Built-in commands and custom commands from `windmenu.toml` are available on the first keypress; Start Menu shortcuts and Windows Store apps populate in the background over the following seconds
- New built-in commands: **Refresh Apps** rescans the Start Menu and Windows Store apps; **Reload Config** reloads custom commands from `windmenu.toml`. Neither requires restarting the daemon

## WLAN scanning removed

- The `WLAN Scan` built-in command and `windmenu test wlan-scan` / `wlan-interfaces` test subcommands are removed. The WLAN API surface (`wlanapi`) was unusual for an application launcher and the WiFi scanning plumbing never materialized into a user-facing feature. The `wlan.rs` module is deleted.
