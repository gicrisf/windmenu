; Windmenu NSIS Installer Script
; This installer bundles windmenu.exe (menu renderer built in since 0.6.0)

!define PRODUCT_NAME "Windmenu"
!ifndef PRODUCT_VERSION
  !define PRODUCT_VERSION "0.0.0"
!endif
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
!define MUI_FINISHPAGE_RUN "$INSTDIR\windmenu.exe"
!define MUI_FINISHPAGE_RUN_PARAMETERS "daemon start"
!define MUI_FINISHPAGE_RUN_TEXT "Start Windmenu Daemon"
!insertmacro MUI_PAGE_FINISH

; Uninstaller pages
!insertmacro MUI_UNPAGE_INSTFILES

; Language files
!insertmacro MUI_LANGUAGE "English"

; Version information
VIProductVersion "1.0.0.0"
VIAddVersionKey "ProductName" "${PRODUCT_NAME}"
VIAddVersionKey "Comments" "Window management utility with daemon"
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
  CreateShortCut "$SMPROGRAMS\${PRODUCT_NAME}\Uninstall.lnk" "$INSTDIR\uninstall.exe"
SectionEnd

SectionGroupEnd

SectionGroup /e "Auto-start Options" SecGrpAutoStart
Section /o "Registry Run (Basic)" SecAutoStartRegistry
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "WindmenuDaemon" '"$INSTDIR\windmenu.exe" daemon start'
SectionEnd

Section /o "Current User Startup Folder" SecAutoStartUser
  ; Plain shortcut; windmenu.exe is a GUI-subsystem binary, so no console flashes
  SetShellVarContext current
  CreateShortCut "$SMSTARTUP\windmenu.lnk" "$INSTDIR\windmenu.exe" "" "$INSTDIR\windmenu.exe" 0 SW_SHOWNORMAL "" "windmenu launcher"
SectionEnd
SectionGroupEnd

; Component descriptions
!insertmacro MUI_FUNCTION_DESCRIPTION_BEGIN
  !insertmacro MUI_DESCRIPTION_TEXT ${SecCore} "Core Windmenu files (windmenu.exe) and configuration files"
  !insertmacro MUI_DESCRIPTION_TEXT ${SecStartMenu} "Create shortcuts in Start Menu"
    !insertmacro MUI_DESCRIPTION_TEXT ${SecAutoStartRegistry} "Start Windmenu automatically using Windows Registry (basic method, starts when current user logs in)"
  !insertmacro MUI_DESCRIPTION_TEXT ${SecAutoStartUser} "Start Windmenu automatically using a shortcut in the current user's Startup folder (no admin required)"
!insertmacro MUI_FUNCTION_DESCRIPTION_END

; Uninstaller section
Section Uninstall
  ; Remove registry keys
  DeleteRegKey ${PRODUCT_UNINST_ROOT_KEY} "${PRODUCT_UNINST_KEY}"
  DeleteRegKey HKCU "${PRODUCT_DIR_REGKEY}"
  
  ; Remove all possible startup methods
  ; Registry Run method
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "WindmenuDaemon"
  
  ; Startup folder method
  SetShellVarContext current
  Delete "$SMSTARTUP\windmenu.lnk"

  ; Remove files and uninstaller
  Delete "$INSTDIR\windmenu.exe"
  Delete "$INSTDIR\uninstall.exe"
  
  ; Remove shortcuts
  Delete "$SMPROGRAMS\${PRODUCT_NAME}\*.*"
  RMDir "$SMPROGRAMS\${PRODUCT_NAME}"
  
  ; Remove directories if empty
  RMDir "$INSTDIR"
  
  SetAutoClose true
SectionEnd

Function .onInit
  MessageBox MB_YESNO|MB_ICONQUESTION \
    "Installer will close any running Windmenu processes to continue. Continue?" \
    IDYES close_processes IDNO abort_install

  abort_install:
    Abort
    
  close_processes:
    IfFileExists "$INSTDIR\windmenu.exe" 0 force_kill
    nsExec::ExecToLog '"$INSTDIR\windmenu.exe" daemon stop'
    Pop $0
  force_kill:
    nsExec::ExecToLog 'taskkill /F /IM windmenu.exe'
FunctionEnd

; Function to handle component selection changes
Function .onSelChange
  ; Count how many startup methods are selected
  StrCpy $0 0
  
  ${If} ${SectionIsSelected} ${SecAutoStartRegistry}
    IntOp $0 $0 + 1
  ${EndIf}
  ${If} ${SectionIsSelected} ${SecAutoStartUser}
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
