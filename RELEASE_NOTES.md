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
