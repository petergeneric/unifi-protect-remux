; UBV Remux NSIS Installer Script
; Builds a Windows installer for the UBV Remux tool suite.
;
; Expected directory layout at build time (set via STAGING_DIR define):
;   staging/
;   ├── remux.exe
;   ├── ubv-info.exe
;   ├── ubv-anonymise.exe
;   ├── *.dll              (FFmpeg shared libs)
;   └── RemuxGui/
;       ├── RemuxGui.exe
;       └── ...
;
; Build:
;   makensis /DSTAGING_DIR=staging /DVERSION=4.1.4 installer\windows.nsi

;-----------------------------------------------------------------------------
; Build-time defines with defaults
;-----------------------------------------------------------------------------
!ifndef STAGING_DIR
  !define STAGING_DIR "staging"
!endif

!ifndef VERSION
  !define VERSION "0.0.0"
!endif

!define PRODUCT_NAME "UBV Remux"
!define PRODUCT_PUBLISHER "Peter Wright"
!define PRODUCT_WEB_SITE "https://github.com/peterwright/unifi-protect-remux"
!define UNINSTALL_REG_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT_NAME}"

;-----------------------------------------------------------------------------
; General
;-----------------------------------------------------------------------------
Name "${PRODUCT_NAME} ${VERSION}"
OutFile "unifi-protect-remux-windows-x86_64-setup.exe"
InstallDir "$PROGRAMFILES64\UBV Remux"
InstallDirRegKey HKLM "${UNINSTALL_REG_KEY}" "InstallLocation"
RequestExecutionLevel admin
SetCompressor /SOLID lzma

;-----------------------------------------------------------------------------
; Version information embedded in the .exe
;-----------------------------------------------------------------------------
VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName" "${PRODUCT_NAME}"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "FileDescription" "${PRODUCT_NAME} Installer"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "LegalCopyright" "AGPL-3.0-only"

;-----------------------------------------------------------------------------
; MUI2 configuration
;-----------------------------------------------------------------------------
!include "MUI2.nsh"
!include "FileFunc.nsh"

!define MUI_ICON "..\assets\appicon.ico"
!define MUI_UNICON "..\assets\appicon.ico"
!define MUI_ABORTWARNING

; Installer pages
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

; Uninstaller pages
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"

;-----------------------------------------------------------------------------
; EnVar plugin for safe PATH manipulation
; https://nsis.sourceforge.io/EnVar_plug-in (zlib license)
;-----------------------------------------------------------------------------

;-----------------------------------------------------------------------------
; Installer sections
;-----------------------------------------------------------------------------

; Core files — always installed (not optional)
Section "-Core Files" SEC_CORE
  SetOutPath "$INSTDIR"

  ; CLI executables
  File "${STAGING_DIR}\remux.exe"
  File "${STAGING_DIR}\ubv-info.exe"
  File "${STAGING_DIR}\ubv-anonymise.exe"

  ; FFmpeg shared libraries
  File "${STAGING_DIR}\*.dll"

  ; Uninstaller
  WriteUninstaller "$INSTDIR\Uninstall.exe"

  ; Add/Remove Programs registry entries
  WriteRegStr HKLM "${UNINSTALL_REG_KEY}" "DisplayName" "${PRODUCT_NAME} ${VERSION}"
  WriteRegStr HKLM "${UNINSTALL_REG_KEY}" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegStr HKLM "${UNINSTALL_REG_KEY}" "QuietUninstallString" '"$INSTDIR\Uninstall.exe" /S'
  WriteRegStr HKLM "${UNINSTALL_REG_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKLM "${UNINSTALL_REG_KEY}" "Publisher" "${PRODUCT_PUBLISHER}"
  WriteRegStr HKLM "${UNINSTALL_REG_KEY}" "URLInfoAbout" "${PRODUCT_WEB_SITE}"
  WriteRegStr HKLM "${UNINSTALL_REG_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegDWORD HKLM "${UNINSTALL_REG_KEY}" "NoModify" 1
  WriteRegDWORD HKLM "${UNINSTALL_REG_KEY}" "NoRepair" 1

  ; Estimate installed size (KB) for Add/Remove Programs
  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  IntFmt $0 "0x%08X" $0
  WriteRegDWORD HKLM "${UNINSTALL_REG_KEY}" "EstimatedSize" $0
SectionEnd

; Component 1: Remux GUI
Section "Remux GUI" SEC_GUI
  SetOutPath "$INSTDIR\RemuxGui"
  File /r "${STAGING_DIR}\RemuxGui\*.*"

  ; Start Menu shortcut
  CreateDirectory "$SMPROGRAMS\${PRODUCT_NAME}"
  CreateShortcut "$SMPROGRAMS\${PRODUCT_NAME}\UBV Remux.lnk" "$INSTDIR\RemuxGui\RemuxGui.exe" "" "$INSTDIR\RemuxGui\RemuxGui.exe" 0
  CreateShortcut "$SMPROGRAMS\${PRODUCT_NAME}\Uninstall.lnk" "$INSTDIR\Uninstall.exe" "" "$INSTDIR\Uninstall.exe" 0
SectionEnd

; Component 2: Add to PATH
Section "Add to PATH" SEC_PATH
  ; Modify current user's PATH
  EnVar::SetHKCU
  EnVar::AddValue "PATH" "$INSTDIR"
  EnVar::AddValue "PATH" "$INSTDIR\RemuxGui"

  ; Record that we modified PATH so the uninstaller knows
  WriteRegStr HKLM "${UNINSTALL_REG_KEY}" "AddedToPath" "1"
SectionEnd

;-----------------------------------------------------------------------------
; Component descriptions
;-----------------------------------------------------------------------------
!insertmacro MUI_FUNCTION_DESCRIPTION_BEGIN
  !insertmacro MUI_DESCRIPTION_TEXT ${SEC_GUI} "Install the graphical interface for UBV Remux. Creates a Start Menu shortcut."
  !insertmacro MUI_DESCRIPTION_TEXT ${SEC_PATH} "Add the install directory to your user PATH so CLI tools (remux, ubv-info, ubv-anonymise) can be run from any terminal."
!insertmacro MUI_FUNCTION_DESCRIPTION_END

;-----------------------------------------------------------------------------
; Uninstaller
;-----------------------------------------------------------------------------
Section "Uninstall"
  ; Remove PATH entries if they were added during install
  ReadRegStr $0 HKLM "${UNINSTALL_REG_KEY}" "AddedToPath"
  StrCmp $0 "1" 0 +4
    EnVar::SetHKCU
    EnVar::DeleteValue "PATH" "$INSTDIR\RemuxGui"
    EnVar::DeleteValue "PATH" "$INSTDIR"

  ; Remove Start Menu shortcuts
  Delete "$SMPROGRAMS\${PRODUCT_NAME}\UBV Remux.lnk"
  Delete "$SMPROGRAMS\${PRODUCT_NAME}\Uninstall.lnk"
  RMDir "$SMPROGRAMS\${PRODUCT_NAME}"

  ; Remove GUI files
  RMDir /r "$INSTDIR\RemuxGui"

  ; Remove core files
  Delete "$INSTDIR\remux.exe"
  Delete "$INSTDIR\ubv-info.exe"
  Delete "$INSTDIR\ubv-anonymise.exe"
  Delete "$INSTDIR\*.dll"
  Delete "$INSTDIR\Uninstall.exe"

  ; Remove install directory (only if empty)
  RMDir "$INSTDIR"

  ; Remove registry entries
  DeleteRegKey HKLM "${UNINSTALL_REG_KEY}"
SectionEnd
