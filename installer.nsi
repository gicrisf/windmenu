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
Section "Core Files (required)" SecCore
  SectionIn RO
  
  ; Set output path to the installation directory
  SetOutPath "$INSTDIR"
  
  ; Install main binaries
  File "target\release\windmenu.exe"
  File "target\release\windmenu-monitor.exe"
  File "assets\wlines-daemon.exe"
  
  ; Install configuration files
  File "config.toml"
  File "assets\daemon-config.txt"
  File "assets\start-wlines-daemon.bat"
  
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

Section "Start Menu Shortcuts" SecStartMenu
  CreateDirectory "$SMPROGRAMS\${PRODUCT_NAME}"
  CreateShortCut "$SMPROGRAMS\${PRODUCT_NAME}\Windmenu Monitor.lnk" "$INSTDIR\windmenu-monitor.exe"
  CreateShortCut "$SMPROGRAMS\${PRODUCT_NAME}\Uninstall.lnk" "$INSTDIR\uninstall.exe"
SectionEnd

Section "Desktop Shortcut" SecDesktop
  CreateShortCut "$DESKTOP\Windmenu Monitor.lnk" "$INSTDIR\windmenu-monitor.exe"
SectionEnd

Section "Auto-start with Windows" SecAutoStart
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "WindmenuDaemon" "$INSTDIR\windmenu.exe"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "WlinesDaemon" "$INSTDIR\start-wlines-daemon.bat"
SectionEnd

; Component descriptions
!insertmacro MUI_FUNCTION_DESCRIPTION_BEGIN
  !insertmacro MUI_DESCRIPTION_TEXT ${SecCore} "Core Windmenu files (windmenu.exe, windmenu-monitor.exe, wlines-daemon.exe), configuration files, and startup scripts"
  !insertmacro MUI_DESCRIPTION_TEXT ${SecStartMenu} "Create shortcuts in Start Menu"
  !insertmacro MUI_DESCRIPTION_TEXT ${SecDesktop} "Create desktop shortcut for Windmenu Monitor"
  !insertmacro MUI_DESCRIPTION_TEXT ${SecAutoStart} "Start Windmenu automatically when Windows starts"
!insertmacro MUI_FUNCTION_DESCRIPTION_END

; Uninstaller section
Section Uninstall
  ; Remove registry keys
  DeleteRegKey ${PRODUCT_UNINST_ROOT_KEY} "${PRODUCT_UNINST_KEY}"
  DeleteRegKey HKCU "${PRODUCT_DIR_REGKEY}"
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "WindmenuDaemon"
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "WlinesDaemon"
  
  ; Remove files and uninstaller
  Delete "$INSTDIR\windmenu.exe"
  Delete "$INSTDIR\windmenu-monitor.exe"
  Delete "$INSTDIR\wlines-daemon.exe"
  Delete "$INSTDIR\config.toml"
  Delete "$INSTDIR\daemon-config.txt"
  Delete "$INSTDIR\start-wlines-daemon.bat"
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
  ; Check if windmenu or windmenu-monitor is running
  System::Call 'kernel32::OpenMutex(i 0x100000, b 0, t "WindmenuMutex") i .R0'
  IntCmp $R0 0 notRunning
    System::Call 'kernel32::CloseHandle(i $R0)'
    MessageBox MB_OK|MB_ICONEXCLAMATION "Windmenu is currently running. Please close it before installing."
    Abort
  notRunning:
FunctionEnd

Function un.onInit
  MessageBox MB_ICONQUESTION|MB_YESNO|MB_DEFBUTTON2 "Are you sure you want to completely remove $(^Name) and all of its components?" IDYES +2
  Abort
FunctionEnd

Function un.onUninstSuccess
  HideWindow
  MessageBox MB_ICONINFORMATION|MB_OK "$(^Name) was successfully removed from your computer."
FunctionEnd
