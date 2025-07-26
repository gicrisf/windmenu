; Windmenu NSIS Installer Script
; This installer bundles windmenu.exe, windmenu-monitor.exe, and wlines-daemon.exe

!define PRODUCT_NAME "Windmenu"
!define PRODUCT_VERSION "0.1.0"
!define PRODUCT_PUBLISHER "Giovanni Crisalfi"
!define PRODUCT_WEB_SITE "https://github.com/gicrisf/windmenu"
!define PRODUCT_DIR_REGKEY "Software\Microsoft\Windows\CurrentVersion\App Paths\windmenu.exe"
!define PRODUCT_UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT_NAME}"
!define PRODUCT_UNINST_ROOT_KEY "HKLM"

; Modern UI
!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "x64.nsh"

; General settings
Name "${PRODUCT_NAME} ${PRODUCT_VERSION}"
OutFile "windmenu-installer.exe"
InstallDir "$LOCALAPPDATA\windmenu"
InstallDirRegKey HKCU "${PRODUCT_DIR_REGKEY}" ""
ShowInstDetails show
ShowUnInstDetails show

; Request application privileges for Windows Vista/7/8/10/11
RequestExecutionLevel user

; Interface Settings
!define MUI_ABORTWARNING
!define MUI_ICON "${NSISDIR}\Contrib\Graphics\Icons\modern-install.ico"
!define MUI_UNICON "${NSISDIR}\Contrib\Graphics\Icons\modern-uninstall.ico"

; Welcome page
!insertmacro MUI_PAGE_WELCOME

; Components page
!insertmacro MUI_PAGE_COMPONENTS

; Directory page
!insertmacro MUI_PAGE_DIRECTORY

; Instfiles page
!insertmacro MUI_PAGE_INSTFILES

; Finish page
!define MUI_FINISHPAGE_RUN "$INSTDIR\windmenu-monitor.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Start Windmenu Monitor"
!insertmacro MUI_PAGE_FINISH

; Uninstaller pages
!insertmacro MUI_UNPAGE_INSTFILES

; Language files
!insertmacro MUI_LANGUAGE "English"

; Version information
VIProductVersion "1.0.0.0"
VIAddVersionKey "ProductName" "${PRODUCT_NAME}"
VIAddVersionKey "Comments" "Window management utility with daemon and monitor"
VIAddVersionKey "CompanyName" "${PRODUCT_PUBLISHER}"
VIAddVersionKey "LegalTrademarks" ""
VIAddVersionKey "LegalCopyright" "© ${PRODUCT_PUBLISHER}"
VIAddVersionKey "FileDescription" "${PRODUCT_NAME} Installer"
VIAddVersionKey "FileVersion" "${PRODUCT_VERSION}"

; Installation sections
SectionGroup "Windmenu Core" SecGrpCore
Section "Core Files (required)" SecCore
  SectionIn RO
  
  ; Set output path to the installation directory
  SetOutPath "$INSTDIR"
  
  ; Install main binaries
  File "target\release\windmenu.exe"
  File "target\release\windmenu-monitor.exe"
  File "assets\wlines-daemon.exe"
  
  ; Install configuration files
  File "windmenu.toml"
  File "assets\wlines-config.txt"
  
  ; Create uninstaller
  WriteUninstaller "$INSTDIR\uninstall.exe"
  
  ; Registry entries
  WriteRegStr HKCU "${PRODUCT_DIR_REGKEY}" "" "$INSTDIR\windmenu.exe"
  WriteRegStr ${PRODUCT_UNINST_ROOT_KEY} "${PRODUCT_UNINST_KEY}" "DisplayName" "$(^Name)"
  WriteRegStr ${PRODUCT_UNINST_ROOT_KEY} "${PRODUCT_UNINST_KEY}" "UninstallString" "$INSTDIR\uninstall.exe"
  WriteRegStr ${PRODUCT_UNINST_ROOT_KEY} "${PRODUCT_UNINST_KEY}" "DisplayIcon" "$INSTDIR\windmenu.exe"
  WriteRegStr ${PRODUCT_UNINST_ROOT_KEY} "${PRODUCT_UNINST_KEY}" "DisplayVersion" "${PRODUCT_VERSION}"
  WriteRegStr ${PRODUCT_UNINST_ROOT_KEY} "${PRODUCT_UNINST_KEY}" "URLInfoAbout" "${PRODUCT_WEB_SITE}"
  WriteRegStr ${PRODUCT_UNINST_ROOT_KEY} "${PRODUCT_UNINST_KEY}" "Publisher" "${PRODUCT_PUBLISHER}"
SectionEnd
SectionGroupEnd

SectionGroup "Shortcuts" SecGrpShortcuts
Section "Start Menu Shortcuts" SecStartMenu
  CreateDirectory "$SMPROGRAMS\${PRODUCT_NAME}"
  CreateShortCut "$SMPROGRAMS\${PRODUCT_NAME}\Windmenu Monitor.lnk" "$INSTDIR\windmenu-monitor.exe"
  CreateShortCut "$SMPROGRAMS\${PRODUCT_NAME}\Uninstall.lnk" "$INSTDIR\uninstall.exe"
SectionEnd

Section "Desktop Shortcut" SecDesktop
  CreateShortCut "$DESKTOP\Windmenu Monitor.lnk" "$INSTDIR\windmenu-monitor.exe"
SectionEnd
SectionGroupEnd

SectionGroup /e "Auto-start Options" SecGrpAutoStart
Section /o "Registry Run (Basic)" SecAutoStartRegistry
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "WindmenuDaemon" "$INSTDIR\windmenu.exe"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "WlinesDaemon" "$INSTDIR\wlines-daemon.exe"
SectionEnd

Section /o "Task Scheduler (Admin)" SecAutoStartTask
  ; Check current privileges
  UserInfo::GetAccountType
  Pop $0
  ${If} $0 != "Admin"
    MessageBox MB_YESNO|MB_ICONEXCLAMATION \
      "Task Scheduler requires administrator privileges. $\n$\nWould you like to restart the installer as administrator?" \
      IDYES run_as_admin IDNO skip_task
    run_as_admin:
      ExecShell "runas" "$EXEPATH" ""
      Quit
    skip_task:
      SectionSetFlags ${SecAutoStartTask} 0  ; Deselect section
      Return
  ${EndIf}
  ; Create task XML content with correct paths
  FileOpen $0 "$INSTDIR\windmenu-task.xml" w
  FileWrite $0 '<?xml version="1.0" encoding="UTF-16"?>$\r$\n'
  FileWrite $0 '<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">$\r$\n'
  FileWrite $0 '  <RegistrationInfo>$\r$\n'
  FileWrite $0 '    <Date>2025-07-11T00:00:00.0000000</Date>$\r$\n'
  FileWrite $0 '    <Author>windmenu</Author>$\r$\n'
  FileWrite $0 '    <Description>Automatically start windmenu application on user login</Description>$\r$\n'
  FileWrite $0 '  </RegistrationInfo>$\r$\n'
  FileWrite $0 '  <Triggers>$\r$\n'
  FileWrite $0 '    <LogonTrigger>$\r$\n'
  FileWrite $0 '      <Enabled>true</Enabled>$\r$\n'
  FileWrite $0 '      <Delay>PT10S</Delay>$\r$\n'
  FileWrite $0 '    </LogonTrigger>$\r$\n'
  FileWrite $0 '  </Triggers>$\r$\n'
  FileWrite $0 '  <Principals>$\r$\n'
  FileWrite $0 '    <Principal id="Author">$\r$\n'
  FileWrite $0 '      <LogonType>InteractiveToken</LogonType>$\r$\n'
  FileWrite $0 '      <RunLevel>LeastPrivilege</RunLevel>$\r$\n'
  FileWrite $0 '    </Principal>$\r$\n'
  FileWrite $0 '  </Principals>$\r$\n'
  FileWrite $0 '  <Settings>$\r$\n'
  FileWrite $0 '    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>$\r$\n'
  FileWrite $0 '    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>$\r$\n'
  FileWrite $0 '    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>$\r$\n'
  FileWrite $0 '    <AllowHardTerminate>true</AllowHardTerminate>$\r$\n'
  FileWrite $0 '    <StartWhenAvailable>true</StartWhenAvailable>$\r$\n'
  FileWrite $0 '    <RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>$\r$\n'
  FileWrite $0 '    <IdleSettings>$\r$\n'
  FileWrite $0 '      <StopOnIdleEnd>false</StopOnIdleEnd>$\r$\n'
  FileWrite $0 '      <RestartOnIdle>false</RestartOnIdle>$\r$\n'
  FileWrite $0 '    </IdleSettings>$\r$\n'
  FileWrite $0 '    <AllowStartOnDemand>true</AllowStartOnDemand>$\r$\n'
  FileWrite $0 '    <Enabled>true</Enabled>$\r$\n'
  FileWrite $0 '    <Hidden>false</Hidden>$\r$\n'
  FileWrite $0 '    <RunOnlyIfIdle>false</RunOnlyIfIdle>$\r$\n'
  FileWrite $0 '    <WakeToRun>false</WakeToRun>$\r$\n'
  FileWrite $0 '    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>$\r$\n'
  FileWrite $0 '    <Priority>7</Priority>$\r$\n'
  FileWrite $0 '  </Settings>$\r$\n'
  FileWrite $0 '  <Actions Context="Author">$\r$\n'
  FileWrite $0 '    <Exec>$\r$\n'
  FileWrite $0 '      <Command>$INSTDIR\windmenu.exe</Command>$\r$\n'
  FileWrite $0 '      <WorkingDirectory>$INSTDIR</WorkingDirectory>$\r$\n'
  FileWrite $0 '    </Exec>$\r$\n'
  FileWrite $0 '    <Exec>$\r$\n'
  FileWrite $0 '      <Command>$INSTDIR\wlines-daemon.exe</Command>$\r$\n'
  FileWrite $0 '      <WorkingDirectory>$INSTDIR</WorkingDirectory>$\r$\n'
  FileWrite $0 '    </Exec>$\r$\n'
  FileWrite $0 '  </Actions>$\r$\n'
  FileWrite $0 '</Task>$\r$\n'
  FileClose $0
  
  ; Create the scheduled task
  nsExec::ExecToLog 'schtasks /create /tn "windmenu" /xml "$INSTDIR\windmenu-task.xml" /f'
  Pop $0
  ${If} $0 != 0
    DetailPrint "Warning: Failed to create scheduled task. You may need to run as administrator."
  ${Else}
    DetailPrint "Scheduled task created successfully"
  ${EndIf}
  
  ; Clean up temporary XML file
  Delete "$INSTDIR\windmenu-task.xml"
SectionEnd

Section /o "Current User Startup Folder" SecAutoStartUser
  ; Create VBS script for silent startup
  FileOpen $0 "$INSTDIR\start-windmenu-user.vbs" w
  FileWrite $0 'Set WshShell = CreateObject("WScript.Shell")$\r$\n'
  FileWrite $0 'WshShell.Run """$INSTDIR\windmenu.exe""", 0, False$\r$\n'
  FileClose $0
  
  ; Copy to user startup folder
  CopyFiles "$INSTDIR\start-windmenu-user.vbs" "$APPDATA\Microsoft\Windows\Start Menu\Programs\Startup\"
SectionEnd

Section /o "All Users Startup Folder" SecAutoStartAll
  ; Create VBS script for silent startup
  FileOpen $0 "$INSTDIR\start-windmenu-all.vbs" w
  FileWrite $0 'Set WshShell = CreateObject("WScript.Shell")$\r$\n'
  FileWrite $0 'WshShell.Run """$INSTDIR\windmenu.exe""", 0, False$\r$\n'
  FileClose $0
  
  ; Copy to all users startup folder (requires admin privileges)
  CopyFiles "$INSTDIR\start-windmenu-all.vbs" "$ALLUSERSPROFILE\Microsoft\Windows\Start Menu\Programs\Startup\"
SectionEnd
SectionGroupEnd

; Component descriptions
!insertmacro MUI_FUNCTION_DESCRIPTION_BEGIN
  !insertmacro MUI_DESCRIPTION_TEXT ${SecCore} "Core Windmenu files (windmenu.exe, windmenu-monitor.exe, wlines-daemon.exe), configuration files, and startup scripts"
  !insertmacro MUI_DESCRIPTION_TEXT ${SecStartMenu} "Create shortcuts in Start Menu"
  !insertmacro MUI_DESCRIPTION_TEXT ${SecDesktop} "Create desktop shortcut for Windmenu Monitor"
  !insertmacro MUI_DESCRIPTION_TEXT ${SecAutoStartRegistry} "Start Windmenu automatically using Windows Registry (basic method, starts when current user logs in)"
  !insertmacro MUI_DESCRIPTION_TEXT ${SecAutoStartTask} "Start Windmenu automatically using Task Scheduler (recommended - most reliable, but needs admin privileges)"
  !insertmacro MUI_DESCRIPTION_TEXT ${SecAutoStartUser} "Start Windmenu automatically using current user's startup folder"
  !insertmacro MUI_DESCRIPTION_TEXT ${SecAutoStartAll} "Start Windmenu automatically using all users startup folder (affects all users on this computer)"
!insertmacro MUI_FUNCTION_DESCRIPTION_END

; Uninstaller section
Section Uninstall
  ; Remove registry keys
  DeleteRegKey ${PRODUCT_UNINST_ROOT_KEY} "${PRODUCT_UNINST_KEY}"
  DeleteRegKey HKCU "${PRODUCT_DIR_REGKEY}"
  
  ; Remove all possible startup methods
  ; Registry Run method
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "WindmenuDaemon"
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "WlinesDaemon"
  
  ; Task Scheduler method
  nsExec::ExecToLog 'schtasks /delete /tn "windmenu" /f'
  Pop $0 ; ignore return value
  
  ; Startup folder methods
  Delete "$APPDATA\Microsoft\Windows\Start Menu\Programs\Startup\start-windmenu.vbs"
  Delete "$ALLUSERSPROFILE\Microsoft\Windows\Start Menu\Programs\Startup\start-windmenu.vbs"
  
  ; Remove files and uninstaller
  Delete "$INSTDIR\windmenu.exe"
  Delete "$INSTDIR\windmenu-monitor.exe"
  Delete "$INSTDIR\wlines-daemon.exe"
  Delete "$INSTDIR\windmenu.toml"
  Delete "$INSTDIR\wlines-config.txt"
  Delete "$INSTDIR\start-windmenu-user.vbs"
  Delete "$INSTDIR\start-windmenu-all.vbs"
  Delete "$INSTDIR\uninstall.exe"
  
  ; Remove shortcuts
  Delete "$SMPROGRAMS\${PRODUCT_NAME}\*.*"
  RMDir "$SMPROGRAMS\${PRODUCT_NAME}"
  Delete "$DESKTOP\Windmenu Monitor.lnk"
  
  ; Remove directories if empty
  RMDir "$INSTDIR"
  
  SetAutoClose true
SectionEnd

; Function to check if application is running
Function .onInit
  MessageBox MB_YESNO|MB_ICONQUESTION \
    "Installer will close any running Windmenu processes to continue. Continue?" \
    IDYES close_processes IDNO abort_install

  abort_install:
    Abort
    
  close_processes:
    nsExec::ExecToLog 'taskkill /F /IM windmenu.exe /IM windmenu-monitor.exe' 
FunctionEnd

; Function to handle component selection changes
Function .onSelChange
  ; Count how many startup methods are selected
  StrCpy $0 0
  
  ${If} ${SectionIsSelected} ${SecAutoStartRegistry}
    IntOp $0 $0 + 1
  ${EndIf}
  ${If} ${SectionIsSelected} ${SecAutoStartTask}
    IntOp $0 $0 + 1
  ${EndIf}
  ${If} ${SectionIsSelected} ${SecAutoStartUser}
    IntOp $0 $0 + 1
  ${EndIf}
  ${If} ${SectionIsSelected} ${SecAutoStartAll}
    IntOp $0 $0 + 1
  ${EndIf}
  
  ; If more than one startup method is selected, show warning
  ${If} $0 > 1
    MessageBox MB_OK|MB_ICONINFORMATION "Warning: You have selected multiple startup methods. Only one should be selected to avoid conflicts. Please select only your preferred startup method."
  ${EndIf}
FunctionEnd

Function un.onInit
  MessageBox MB_ICONQUESTION|MB_YESNO|MB_DEFBUTTON2 "Are you sure you want to completely remove $(^Name) and all of its components?" IDYES +2
  Abort
FunctionEnd

Function un.onUninstSuccess
  HideWindow
  MessageBox MB_ICONINFORMATION|MB_OK "$(^Name) was successfully removed from your computer."
FunctionEnd
