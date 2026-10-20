## Supervised auto-start

- Auto-start now uses a per-user Scheduled Task (`\Windmenu\Autorun for <user>`) triggered at logon with a 3-second delay: specific-user logon trigger, `InteractiveToken`, least privilege, batteries allowed, no execution time limit, `StartWhenAvailable=false`, no restart-on-failure. If Task Scheduler is blocked, `autostart.ps1` warns and falls back to the Startup-folder shortcut
- The task launches a new lightweight supervisor (`windmenu.exe --start-supervisor`), which starts the ordinary daemon and respawns it if it exits with a failure. The daemon keeps its single-process behavior: `windmenu start` / `stop` / `restart` are unchanged, and `windmenu stop` also stops the supervisor so it cannot resurrect the daemon
- `--start-supervisor` is an internal flag; the daemon never opens the supervisor mutex. `windmenu doctor` reports the supervisor and Scheduled Task state, flags a stale task executable path, and points at `autostart.ps1` for the exact enable/disable command
- Hotkey registration at startup now retries for 60 seconds when another app briefly holds the combo, instead of failing immediately and showing an error box

## Flatter CLI (0.7.0)

- Daemon commands are now top-level: `windmenu start` / `stop` / `restart` (previously nested under `windmenu daemon …`). Bare `windmenu` now prints help instead of starting the daemon — use `windmenu start`
- **Breaking:** the `windmenu daemon …` subcommand group is gone — update any scripts, shortcuts, or Run-key entries to the flat form
- New `windmenu doctor` replaces `windmenu status` and `windmenu config path`: one command reports config resolution + warnings, the binary location and whether it's on PATH, daemon running state, and the active auto-start method (Startup-folder shortcut and a read-only probe of the `HKCU\…\Run\WindmenuDaemon` registry key). **Breaking:** `windmenu status` and `windmenu config path` are removed

## Auto-start moved out of the app (0.7.0)

- windmenu no longer manages auto-start itself: the `windmenu daemon enable/disable` commands and the built-in Registry/Startup-folder logic (and the `mslnk` dependency) have been removed. `windmenu doctor` reports the daemon running state and the auto-start method in effect
- Auto-start is now a plain Startup-folder shortcut (or registry Run-key entry) you create once. The Scoop and PowerShell installers set it up for you; otherwise the bundled `autostart.ps1` helper (enable/disable/status) does it, and `AUTOSTART.md` documents the by-hand PowerShell
- The NSIS installer has been dropped — releases ship a single `windmenu.zip` (windmenu.exe + autostart.ps1 + config)

## Horizontal bar mode (0.7.0)

- New `horizontal = true` setting renders a single-row bar at the top of the screen, dmenu-style. Entries flow left-to-right, packed into greedy pages with `<` / `>` markers when they overflow. Navigation is via Left/Right arrows (edge-triggered: they move the selection once the caret can't travel further)
- Centering is now a separate `center` option; setting `width` no longer forces centering. `center = false` pins the window to the monitor top-left
- Backward compatible — `horizontal = false` (the default) keeps the classic vertical layout unchanged

## Selection-frequency history (0.7.0)

- Entries you launch more often now rise to the top of the menu automatically. The fuzzy sorter tie-breaks by list position, so history also decides between equally-scored fuzzy matches
- New `history = false` config flag to disable (default: `true`; already the default in the shipped `windmenu.toml`, commented out)
- History is persisted to `windmenu_history.txt` next to the config file (or next to `windmenu.exe` when no config exists) — plain `count<TAB>name` lines, easy to inspect or edit. Top 500 entries kept; the long tail drops on save
- Crash-safe writes: goes through a temp file + rename so a mid-save crash can't truncate history
- No file is created until the first launch; history is a convenience and errors are silently ignored

## Bundled packs removed

- `config pack list` / `install` / `show` are removed. The bundled theme and
  command packs no longer ship in the binary. (The `import` system and `[themes.*]`
  loading are unchanged; your config with `import = [...]` lines still works.)
- **Breaking:** `windmenu config pack …` subcommands are gone.
