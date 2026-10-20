<#
.SYNOPSIS
    Enable, disable, or check windmenu's auto-start on login.

.DESCRIPTION
    Three mechanisms are supported:
      Task     - a per-user Scheduled Task (default; runs at logon +3s)
      Startup  - a shortcut in the current user's Startup folder (no admin)
      Registry - an HKCU\...\Run entry named "WindmenuDaemon"

    'enable' switches to the chosen method and clears the others, so exactly
    one is ever active. If Task creation is blocked (permissions or policy),
    it warns and falls back to the Startup shortcut. 'disable' removes all.

    The managed process is `windmenu.exe --start-supervisor`, which keeps the
    daemon alive across crashes. `windmenu start` still starts a single daemon
    directly and is unaffected.

.EXAMPLE
    powershell -NoProfile -ExecutionPolicy Bypass -File autostart.ps1 enable
    powershell -NoProfile -ExecutionPolicy Bypass -File autostart.ps1 enable -Method Startup
    powershell -NoProfile -ExecutionPolicy Bypass -File autostart.ps1 disable
    powershell -NoProfile -ExecutionPolicy Bypass -File autostart.ps1 status
#>
[CmdletBinding()]
param(
    [Parameter(Position = 0)]
    [ValidateSet('enable', 'disable', 'status')]
    [string]$Action = 'status',

    [ValidateSet('Task', 'Startup', 'Registry')]
    [string]$Method = 'Task',

    # Path to windmenu.exe. Auto-detected if omitted (sibling of this script, then PATH).
    [string]$ExePath
)

$ErrorActionPreference = 'Stop'

$RunName  = 'WindmenuDaemon'
$RunKey   = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$LnkPath  = Join-Path ([Environment]::GetFolderPath('Startup')) 'windmenu.lnk'
$UserName = [Environment]::UserName
$UserId   = [System.Security.Principal.WindowsIdentity]::GetCurrent().Name
$TaskName = "Autorun for $UserName"
$TaskFolder = '\Windmenu\'
$SupervisorArg = '--start-supervisor'

function Resolve-Exe {
    if ($ExePath) {
        if (-not (Test-Path $ExePath)) { throw "windmenu.exe not found at: $ExePath" }
        return (Resolve-Path $ExePath).Path
    }
    $sibling = Join-Path $PSScriptRoot 'windmenu.exe'
    if (Test-Path $sibling) { return $sibling }
    $cmd = Get-Command windmenu -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }
    throw "Could not locate windmenu.exe. Pass -ExePath explicitly."
}

function Remove-StartupLnk {
    if (Test-Path $LnkPath) {
        Remove-Item $LnkPath -Force
        return $true
    }
    return $false
}

function Remove-RunKey {
    if (Get-ItemProperty -Path $RunKey -Name $RunName -ErrorAction SilentlyContinue) {
        Remove-ItemProperty -Path $RunKey -Name $RunName
        return $true
    }
    return $false
}

# Task Scheduler names are per-folder. Remove the task from one or both known
# paths; $null means both (used when switching methods and on disable).
function Remove-TaskAt {
    param([string]$Name, [string]$Path)
    $task = $null
    try {
        $task = Get-ScheduledTask -TaskName $Name -TaskPath $Path -ErrorAction Stop
    } catch {
        # Missing folder or no permission to read it: nothing to remove.
    }
    if ($task) {
        Unregister-ScheduledTask -TaskName $Name -TaskPath $Path -Confirm:$false | Out-Null
        return $true
    }
    return $false
}

function Remove-Task {
    param([string[]]$Paths = @($TaskFolder, '\'))
    $removed = $false
    foreach ($path in $Paths) {
        if (Remove-TaskAt $TaskName $path) { $removed = $true }
    }
    # Old releases wrote root tasks named "windmenu" (NSIS) or
    # "windmenu-daemon" (Rust CLI). Clear them on enable/disable.
    foreach ($legacy in @('windmenu-daemon', 'windmenu')) {
        foreach ($path in @($TaskFolder, '\')) {
            if (Remove-TaskAt $legacy $path) { $removed = $true }
        }
    }
    return $removed
}

# The task lives in a dedicated folder. Folder creation needs
# write access to the Task Scheduler root; if that is denied, use the root
# folder with the same per-user task name.
function Resolve-TaskPath {
    try {
        $service = New-Object -ComObject Schedule.Service
        $service.Connect()
        try {
            $null = $service.GetFolder($TaskFolder)
        } catch {
            $null = $service.GetFolder('\').CreateFolder('Windmenu')
        }
        return $TaskFolder
    } catch {
        Write-Warning "Could not create the $TaskFolder task folder ($($_.Exception.Message)); using the root folder."
        return '\'
    }
}

function Enable-Startup {
    $exe = Resolve-Exe
    $ws  = New-Object -ComObject WScript.Shell
    $s   = $ws.CreateShortcut($LnkPath)
    $s.TargetPath       = $exe
    $s.Arguments        = $SupervisorArg
    $s.WorkingDirectory = Split-Path $exe
    $s.Save()
    [void](Remove-RunKey)
    [void](Remove-Task)
    Write-Host "Enabled: Startup shortcut -> $LnkPath" -ForegroundColor Green
}

function Enable-Registry {
    $exe = Resolve-Exe
    Set-ItemProperty -Path $RunKey -Name $RunName -Value "`"$exe`" $SupervisorArg"
    [void](Remove-StartupLnk)
    [void](Remove-Task)
    Write-Host "Enabled: registry Run key -> $RunKey\$RunName" -ForegroundColor Green
}

function Enable-Task {
    $exe = Resolve-Exe
    $taskPath = Resolve-TaskPath

    $action = New-ScheduledTaskAction -Execute $exe -Argument $SupervisorArg
    $trigger = New-ScheduledTaskTrigger -AtLogOn -User $UserId
    $trigger.Delay = 'PT3S'
    $principal = New-ScheduledTaskPrincipal -UserId $UserId -LogonType Interactive -RunLevel Limited
    # Task settings: no battery gates, no execution time limit,
    # StartWhenAvailable=false, default multiple-instance policy, no restart.
    $settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries `
        -DontStopIfGoingOnBatteries -ExecutionTimeLimit ([TimeSpan]::Zero) `
        -MultipleInstances IgnoreNew

    try {
        Register-ScheduledTask -TaskName $TaskName -TaskPath $taskPath `
            -Action $action -Trigger $trigger -Principal $principal `
            -Settings $settings -Description 'windmenu daemon' -Force | Out-Null
    } catch {
        Write-Warning "Task Scheduler setup failed ($($_.Exception.Message)). Falling back to the Startup shortcut."
        [void](Remove-Task)
        Enable-Startup
        return
    }

    # Exactly one method: clear the other task location and the other methods.
    $otherPath = if ($taskPath -eq '\') { $TaskFolder } else { '\' }
    [void](Remove-Task -Paths @($otherPath))
    [void](Remove-StartupLnk)
    [void](Remove-RunKey)
    Write-Host "Enabled: Scheduled Task -> $taskPath$TaskName (at logon, +3s)" -ForegroundColor Green
}

function Get-TaskInfo {
    foreach ($path in @($TaskFolder, '\')) {
        $task = $null
        try {
            $task = Get-ScheduledTask -TaskName $TaskName -TaskPath $path -ErrorAction Stop
        } catch {
            # Missing folder or no permission to read it.
        }
        if ($task) {
            return [pscustomobject]@{ Path = $path; Task = $task }
        }
    }
    return $null
}

function Disable-All {
    $removed = $false
    if (Remove-StartupLnk) { $removed = $true }
    if (Remove-RunKey) { $removed = $true }
    if (Remove-Task) { $removed = $true }
    if ($removed) {
        Write-Host "Auto-start disabled." -ForegroundColor Green
    } else {
        Write-Host "Auto-start was not enabled; nothing to remove." -ForegroundColor Yellow
    }
}

function Show-Status {
    $info = Get-TaskInfo
    if ($info) {
        $action = $info.Task.Actions | Select-Object -First 1
        $where = "$($info.Path)$TaskName"
        Write-Host "Task Scheduler : present  ($where -> $($action.Execute) $($action.Arguments))"
        if ($action.Arguments -notmatch [regex]::Escape($SupervisorArg)) {
            Write-Host "Warning: task action does not use $SupervisorArg; run 'enable' to refresh it." -ForegroundColor Yellow
        }
    } else {
        Write-Host "Task Scheduler : absent"
    }

    $lnk = Test-Path $LnkPath
    $reg = [bool](Get-ItemProperty -Path $RunKey -Name $RunName -ErrorAction SilentlyContinue)
    Write-Host "Startup shortcut : $(if ($lnk) { 'present' } else { 'absent' })  ($LnkPath)"
    Write-Host "Registry Run key : $(if ($reg) { 'present' } else { 'absent' })  ($RunKey\$RunName)"

    $active = 0
    if ($info) { $active++ }
    if ($lnk)  { $active++ }
    if ($reg)  { $active++ }
    if ($active -gt 1) {
        Write-Host "Warning: more than one mechanism is active; run 'disable' then 'enable'." -ForegroundColor Yellow
    }
}

switch ($Action) {
    'enable' {
        switch ($Method) {
            'Task'     { Enable-Task }
            'Registry' { Enable-Registry }
            default    { Enable-Startup }
        }
    }
    'disable' { Disable-All }
    'status'  { Show-Status }
}
