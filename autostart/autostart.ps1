<#
.SYNOPSIS
    Enable, disable, or check windmenu's auto-start on login.

.DESCRIPTION
    Two mechanisms are supported:
      Startup  - a shortcut in the current user's Startup folder (default, no admin)
      Registry - an HKCU\...\Run entry named "WindmenuDaemon"

    'enable' switches to the chosen method and clears the other, so exactly one
    is ever active. 'disable' removes BOTH, regardless of how it was enabled.

.EXAMPLE
    powershell -NoProfile -ExecutionPolicy Bypass -File autostart.ps1 enable
    powershell -NoProfile -ExecutionPolicy Bypass -File autostart.ps1 enable -Method Registry
    powershell -NoProfile -ExecutionPolicy Bypass -File autostart.ps1 disable
    powershell -NoProfile -ExecutionPolicy Bypass -File autostart.ps1 status
#>
[CmdletBinding()]
param(
    [Parameter(Position = 0)]
    [ValidateSet('enable', 'disable', 'status')]
    [string]$Action = 'status',

    [ValidateSet('Startup', 'Registry')]
    [string]$Method = 'Startup',

    # Path to windmenu.exe. Auto-detected if omitted (sibling of this script, then PATH).
    [string]$ExePath
)

$ErrorActionPreference = 'Stop'

$RunName = 'WindmenuDaemon'
$RunKey  = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$LnkPath = Join-Path ([Environment]::GetFolderPath('Startup')) 'windmenu.lnk'

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

function Enable-Startup {
    $exe = Resolve-Exe
    $ws  = New-Object -ComObject WScript.Shell
    $s   = $ws.CreateShortcut($LnkPath)
    $s.TargetPath       = $exe
    $s.Arguments        = 'start'
    $s.WorkingDirectory = Split-Path $exe
    $s.Save()
    [void](Remove-RunKey)  # enforce single method
    Write-Host "Enabled: Startup shortcut -> $LnkPath" -ForegroundColor Green
}

function Enable-Registry {
    $exe = Resolve-Exe
    Set-ItemProperty -Path $RunKey -Name $RunName -Value "`"$exe`" start"
    [void](Remove-StartupLnk)  # enforce single method
    Write-Host "Enabled: registry Run key -> $RunKey\$RunName" -ForegroundColor Green
}

function Disable-All {
    $removed = (Remove-StartupLnk) -or (Remove-RunKey)
    if ($removed) {
        Write-Host "Auto-start disabled." -ForegroundColor Green
    } else {
        Write-Host "Auto-start was not enabled; nothing to remove." -ForegroundColor Yellow
    }
}

function Show-Status {
    $lnk = Test-Path $LnkPath
    $reg = [bool](Get-ItemProperty -Path $RunKey -Name $RunName -ErrorAction SilentlyContinue)
    Write-Host "Startup shortcut : $(if ($lnk) { 'present' } else { 'absent' })  ($LnkPath)"
    Write-Host "Registry Run key : $(if ($reg) { 'present' } else { 'absent' })  ($RunKey\$RunName)"
    if ($lnk -and $reg) {
        Write-Host "Warning: both mechanisms are active; run 'disable' then 'enable' to keep one." -ForegroundColor Yellow
    }
}

switch ($Action) {
    'enable'  { if ($Method -eq 'Registry') { Enable-Registry } else { Enable-Startup } }
    'disable' { Disable-All }
    'status'  { Show-Status }
}
