# Auto-start on login

windmenu does **not** manage auto-start itself: `windmenu start` just starts the
single daemon process. Starting it automatically at login is a one-time setup
done with a standard Windows mechanism. Three are supported:

| Mechanism | Where it lives                                        | Notes |
|-----------|-------------------------------------------------------|-------|
| Task      | `\Windmenu\Autorun for <user>` (Scheduled Task)       | Default. Runs at logon +3 s, no admin normally |
| Startup   | A `windmenu.lnk` in your Startup folder               | Fallback when Task Scheduler is blocked |
| Registry  | `HKCU\...\CurrentVersion\Run\WindmenuDaemon`          | Alternative to the Startup shortcut |

The Task and Startup/Registry entries launch `windmenu.exe --start-supervisor`,
not `windmenu start`. The supervisor is a tiny external wrapper: it starts the
ordinary daemon and respawns it if it crashes. The daemon keeps its normal
single-process behavior, and `windmenu start` / `stop` / `restart` are unchanged.

The Task Scheduler setup uses a per-user task, specific-user logon
trigger, `PT3S` delay, `InteractiveToken`, least-privilege (`LUA`), batteries
allowed, no execution time limit, `StartWhenAvailable=false`, and no
restart-on-failure (the supervisor handles restarts).

## The `autostart.ps1` helper

If you installed from `windmenu.zip`, `autostart.ps1` ships next to
`windmenu.exe` and handles all three mechanisms for you (it's also what the
Scoop package and the PowerShell installer call under the hood). Run it from the
folder that contains `windmenu.exe` (so it auto-detects the exe):

```powershell
# Enable the Scheduled Task (default; falls back to Startup if blocked)
powershell -NoProfile -ExecutionPolicy Bypass -File .\autostart.ps1 enable

# Explicit alternatives
powershell -NoProfile -ExecutionPolicy Bypass -File .\autostart.ps1 enable -Method Startup
powershell -NoProfile -ExecutionPolicy Bypass -File .\autostart.ps1 enable -Method Registry

# Check what's currently active
powershell -NoProfile -ExecutionPolicy Bypass -File .\autostart.ps1 status

# Disable (removes all three mechanisms)
powershell -NoProfile -ExecutionPolicy Bypass -File .\autostart.ps1 disable
```

`enable` switches to the chosen mechanism and clears the others, so exactly one
is ever active; `disable` removes all three. If `windmenu.exe` isn't beside the
script or on your `PATH`, point at it with `-ExePath 'C:\path\to\windmenu.exe'`.

The task is normally created in the `\Windmenu\` folder. Standard users may be
denied permission to create that folder; in that case the helper uses the root
folder with the same task name. If Task Scheduler itself is blocked, it warns
and creates the Startup shortcut instead.

> `-ExecutionPolicy Bypass` is needed because a script extracted from a
> downloaded zip carries the "mark of the web" and would otherwise be blocked.

## By hand

With a `cargo install` (or any install without the helper), set it up by hand
with the same commands `autostart.ps1` runs. Copy-paste into PowerShell.

These assume `windmenu` is on your `PATH` (it is after `cargo install` or if you
added the install dir). If it isn't, just set `$exe` to the full path instead —
`$exe = 'C:\path\to\windmenu.exe'` and the rest works the same.

**Scheduled Task (recommended):**

```powershell
$exe = (Get-Command windmenu).Source
$userId = [System.Security.Principal.WindowsIdentity]::GetCurrent().Name
$task = 'Autorun for ' + $env:USERNAME
$svc = New-Object -ComObject Schedule.Service; $svc.Connect()
try { $null = $svc.GetFolder('\Windmenu\') } catch { $null = $svc.GetFolder('\').CreateFolder('Windmenu') }

$action = New-ScheduledTaskAction -Execute $exe -Argument '--start-supervisor'
$trigger = New-ScheduledTaskTrigger -AtLogOn -User $userId
$trigger.Delay = 'PT3S'
$principal = New-ScheduledTaskPrincipal -UserId $userId -LogonType Interactive -RunLevel Limited
$settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries `
    -DontStopIfGoingOnBatteries -ExecutionTimeLimit ([TimeSpan]::Zero) `
    -MultipleInstances IgnoreNew

Register-ScheduledTask -TaskName $task -TaskPath '\Windmenu\' `
    -Action $action -Trigger $trigger -Principal $principal `
    -Settings $settings -Description 'windmenu daemon' -Force
```

Remove it:

```powershell
Unregister-ScheduledTask -TaskName ('Autorun for ' + $env:USERNAME) -TaskPath '\Windmenu\' -Confirm:$false
```

If the folder could not be created, use `-TaskPath '\'` instead.

**Startup-folder shortcut:**

```powershell
$exe = (Get-Command windmenu).Source
$lnk = Join-Path ([Environment]::GetFolderPath('Startup')) 'windmenu.lnk'
$s = (New-Object -ComObject WScript.Shell).CreateShortcut($lnk)
$s.TargetPath = $exe; $s.Arguments = '--start-supervisor'; $s.WorkingDirectory = Split-Path $exe; $s.Save()
```

Remove it:

```powershell
Remove-Item (Join-Path ([Environment]::GetFolderPath('Startup')) 'windmenu.lnk') -ErrorAction SilentlyContinue
```

**Registry Run key:**

```powershell
$exe = (Get-Command windmenu).Source
Set-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' WindmenuDaemon "`"$exe`" --start-supervisor"
```

Remove it:

```powershell
Remove-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' WindmenuDaemon
```

## Doctor hints

Run `windmenu doctor`: it reports the active autorun method (Scheduled Task,
Startup shortcut, Registry Run key), the supervisor and daemon states, and flags
a task that points at a stale executable path. When the helper ships next to the
exe, it prints the exact `autostart.ps1 enable` / `disable` command to paste.

## Stopping and restarting

`windmenu stop` stops the supervisor as well as the daemon, so auto-start does
not immediately bring the daemon back. `windmenu start` still starts a bare
daemon (no supervisor); after a manual `windmenu restart`, supervision resumes
at the next logon or when the scheduled task is run again.
