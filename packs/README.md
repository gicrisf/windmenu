# Packs

## Multiple themes

A theme is a named set of the six color keys in a `[themes.<name>]` table.
Define as many as you like in `windmenu.toml` and select one with `theme`:

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

`theme = "default"` selects the built-in scheme (an unknown name falls back to
it too). Top-level color keys still win over the selected theme, so you can pick
a theme and override just one accent.

## Imports

Defining lots of themes (or commands) inline bloats your config. Instead, keep
each in a standalone file and pull it in with `import` (paths are relative to
the config file):

```toml
import = ["packs/catppuccin-theme.toml", "packs/power-commands.toml"]
```

An imported pack is an ordinary TOML file that contains only `[themes.<name>]`
tables and/or `[[commands]]` entries (any other keys are ignored). Importing is
**non-recursive**: a pack cannot import further packs. A missing or malformed
pack is not fatal — windmenu warns and skips it rather than failing to start.

Merge order is predictable: your `windmenu.toml` always wins over imports, and
among imports the last one listed wins. So a `[themes.nord]` or a command named
`Shutdown` in your main file overrides one of the same name from a pack.

The filename is up to you, but the convention is a suffix naming the config
section the pack contributes — `-theme` or `-commands` — so an `import` list
reads at a glance (`packs/gruvbox-theme.toml`, `packs/emacs-commands.toml`).
It's advisory only: windmenu never parses the filename, and a single file may
carry both themes and commands. Keybindings are a kind of command, so describe
them in the *name* (`packs/wt-keys-commands.toml`), not with a new suffix.

## Bundled packs

Windmenu bundles a few ready-made packs. List them, then install one next to
your config:

```bash
windmenu config pack list                # what's available (--themes / --commands to filter)
windmenu config pack install catppuccin  # writes packs/catppuccin-theme.toml + prints the import line
windmenu config pack show catppuccin     # preview without installing
```

With a theme pack installed, add `import` and `theme` to your config:

```toml
import = ["packs/catppuccin-theme.toml"]
theme = "catppuccin-mocha"
```

Command packs need only the `import` line — they add entries to the menu
automatically.

### Available

- `catppuccin` — Catppuccin Frappé and Mocha themes
- `dmenu`, `hatsunemiku`, `nord`, `tokyonight` — more themes
- `power` — shutdown / restart / log off / hibernate / lock
- `windows-tools` — Device Manager, Services, Event Viewer, and other consoles
- `wt-keys` — Windows Terminal tab / pane / font shortcuts
