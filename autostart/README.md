# Auto-start on login

windmenu does **not** manage auto-start itself: the launcher just runs when you
tell it to (`windmenu start`). Starting it automatically at login is a one-time
setup you do with a standard Windows mechanism. There are two:

| Mechanism | Where it lives                               |
|-----------|----------------------------------------------|
| Startup   | A `windmenu.lnk` in your Startup folder      |
| Registry  | `HKCU\...\CurrentVersion\Run\WindmenuDaemon` |

Either one launches `windmenu start` at login. Pick **one**. `windmenu doctor`
reports which (if any) is active, and it looks for exactly the shortcut name
(`windmenu.lnk`) and registry value (`WindmenuDaemon`) used below, so stick to
these and doctor stays accurate.

## The `autostart.ps1` helper

If you installed from `windmenu.zip`, `autostart.ps1` ships next to
`windmenu.exe` and handles both mechanisms for you (it's also what the Scoop
package and the PowerShell installer call under the hood). Run it from the folder
that contains `windmenu.exe` (so it auto-detects the exe):

```powershell
# Enable via the Startup-folder shortcut
powershell -NoProfile -ExecutionPolicy Bypass -File .\autostart.ps1 enable

# Enable via the registry Run key
powershell -NoProfile -ExecutionPolicy Bypass -File .\autostart.ps1 enable -Method Registry

# Check what's currently active
powershell -NoProfile -ExecutionPolicy Bypass -File .\autostart.ps1 status

# Disable (removes both mechanisms)
powershell -NoProfile -ExecutionPolicy Bypass -File .\autostart.ps1 disable
```

`enable` switches to the chosen mechanism and clears the other, so exactly one is
ever active; `disable` removes both. If `windmenu.exe` isn't beside the script or
on your `PATH`, point at it with `-ExePath 'C:\path\to\windmenu.exe'`.

> `-ExecutionPolicy Bypass` is needed because a script extracted from a
> downloaded zip carries the "mark of the web" and would otherwise be blocked.

## By hand

With a `cargo install` (or any install without the helper), set it up by hand
with the same commands `autostart.ps1` runs. Copy-paste into PowerShell.

These assume `windmenu` is on your `PATH` (it is after `cargo install` or if you
added the install dir). If it isn't, just set `$exe` to the full path instead —
`$exe = 'C:\path\to\windmenu.exe'` and the rest works the same.

**Startup-folder shortcut:**

```powershell
$exe = (Get-Command windmenu).Source
$lnk = Join-Path ([Environment]::GetFolderPath('Startup')) 'windmenu.lnk'
$s = (New-Object -ComObject WScript.Shell).CreateShortcut($lnk)
$s.TargetPath = $exe; $s.Arguments = 'start'; $s.WorkingDirectory = Split-Path $exe; $s.Save()
```

Remove it:

```powershell
Remove-Item (Join-Path ([Environment]::GetFolderPath('Startup')) 'windmenu.lnk') -ErrorAction SilentlyContinue
```

**Registry Run key:**

```powershell
$exe = (Get-Command windmenu).Source
Set-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' WindmenuDaemon "`"$exe`" start"
```

Remove it:

```powershell
Remove-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' WindmenuDaemon
```
