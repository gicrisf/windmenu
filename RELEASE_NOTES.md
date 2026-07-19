## Flatter CLI (0.7.0)

- Daemon commands are now top-level: `windmenu start` / `stop` / `restart` / `status` (previously nested under `windmenu daemon …`). Bare `windmenu` now prints help instead of starting the daemon — use `windmenu start`
- **Breaking:** the `windmenu daemon …` subcommand group is gone — update any scripts, shortcuts, or Run-key entries to the flat form

## Auto-start moved out of the app (0.7.0)

- windmenu no longer manages auto-start itself: the `windmenu daemon enable/disable` commands and the built-in Registry/Startup-folder logic (and the `mslnk` dependency) have been removed. `status` now reports only whether the daemon is running
- Auto-start is now a plain Startup-folder shortcut (or registry Run-key entry) you create once. The NSIS installer and the Scoop/PowerShell installers set it up for you; the README documents the two-line PowerShell to enable/disable it manually

## Horizontal bar mode (0.7.0)

- New `horizontal = true` setting renders a single-row bar at the top of the screen, dmenu-style. Entries flow left-to-right, packed into greedy pages with `<` / `>` markers when they overflow. Navigation is via Left/Right arrows (edge-triggered: they move the selection once the caret can't travel further)
- Centering is now a separate `center` option; setting `width` no longer forces centering. `center = false` pins the window to the monitor top-left
- Backward compatible — `horizontal = false` (the default) keeps the classic vertical layout unchanged
