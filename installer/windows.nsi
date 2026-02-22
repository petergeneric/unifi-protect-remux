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
; Installed layout differs — CLI tools go into a cli/ subdirectory so that
; only executables end up on PATH (not Uninstall.exe or icons):
;   $INSTDIR/
;   ├── Uninstall.exe
;   ├── resource/
;   │   └── ubv-document.ico
;   ├── cli/
;   │   ├── remux.exe
;   │   ├── ubv-info.exe
;   │   ├── ubv-anonymise.exe
;   │   └── *.dll
;   └── gui/
;       ├── RemuxGui.exe
;       └── ...
;
; Build:
;   makensis /DSTAGING_DIR=staging /DVERSION=maj.min.rev installer\windows.nsi

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
OutFile "cli-windows-x86_64-setup.exe"
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
  ; CLI executables and FFmpeg DLLs go into a subdirectory so that only
  ; actual tools end up on PATH (not Uninstall.exe or ubv-document.ico).
  SetOutPath "$INSTDIR\cli"

  ; CLI executables
  File "${STAGING_DIR}\remux.exe"
  File "${STAGING_DIR}\ubv-info.exe"
  File "${STAGING_DIR}\ubv-anonymise.exe"

  ; FFmpeg shared libraries
  File "${STAGING_DIR}\*.dll"

  ; Uninstaller (in install root, not on PATH)
  SetOutPath "$INSTDIR"
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
  SetOutPath "$INSTDIR\gui"
  File /r "${STAGING_DIR}\RemuxGui\*.*"

  ; Start Menu shortcut
  CreateDirectory "$SMPROGRAMS\${PRODUCT_NAME}"
  CreateShortcut "$SMPROGRAMS\${PRODUCT_NAME}\UBV Remux.lnk" "$INSTDIR\gui\RemuxGui.exe" "" "$INSTDIR\gui\RemuxGui.exe" 0
  CreateShortcut "$SMPROGRAMS\${PRODUCT_NAME}\Uninstall.lnk" "$INSTDIR\Uninstall.exe" "" "$INSTDIR\Uninstall.exe" 0
SectionEnd

; Component 2: Add to PATH
Section "Add to PATH" SEC_PATH
  ; Modify current user's PATH
  EnVar::SetHKCU
  EnVar::AddValue "PATH" "$INSTDIR\cli"
  EnVar::AddValue "PATH" "$INSTDIR\gui"

  ; Record that we modified PATH so the uninstaller knows
  WriteRegStr HKLM "${UNINSTALL_REG_KEY}" "AddedToPath" "1"
SectionEnd

; Component 3: Associate .ubv files with Remux GUI
Section "Associate .ubv files" SEC_ASSOC
  ; Install the document icon
  SetOutPath "$INSTDIR\resource"
  File "..\assets\ubv-document.ico"

  ; Register the ProgID
  WriteRegStr HKLM "Software\Classes\UBVRemux.ubv" "" "UBV Video Recording"
  WriteRegStr HKLM "Software\Classes\UBVRemux.ubv\DefaultIcon" "" "$INSTDIR\resource\ubv-document.ico"
  WriteRegStr HKLM "Software\Classes\UBVRemux.ubv\shell\open\command" "" '"$INSTDIR\gui\RemuxGui.exe" "%1"'

  ; Associate .ubv extension with our ProgID
  WriteRegStr HKLM "Software\Classes\.ubv" "" "UBVRemux.ubv"

  ; Notify Explorer that file associations have changed
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'

  ; Record that we added the association so the uninstaller knows
  WriteRegStr HKLM "${UNINSTALL_REG_KEY}" "AddedFileAssoc" "1"
SectionEnd

;-----------------------------------------------------------------------------
; Component descriptions
;-----------------------------------------------------------------------------
!insertmacro MUI_FUNCTION_DESCRIPTION_BEGIN
  !insertmacro MUI_DESCRIPTION_TEXT ${SEC_GUI} "Install the graphical interface for UBV Remux. Creates a Start Menu shortcut."
  !insertmacro MUI_DESCRIPTION_TEXT ${SEC_PATH} "Add the install directory to your user PATH so CLI tools (remux, ubv-info, ubv-anonymise) can be run from any terminal."
  !insertmacro MUI_DESCRIPTION_TEXT ${SEC_ASSOC} "Associate .ubv files with Remux GUI so they open in the application when double-clicked."
!insertmacro MUI_FUNCTION_DESCRIPTION_END

;-----------------------------------------------------------------------------
; Uninstaller
;-----------------------------------------------------------------------------
Section "Uninstall"
  ; Remove file association if it was added during install
  ReadRegStr $0 HKLM "${UNINSTALL_REG_KEY}" "AddedFileAssoc"
  StrCmp $0 "1" 0 assoc_done
    ; Only remove .ubv key if it still points to our ProgID
    ReadRegStr $1 HKLM "Software\Classes\.ubv" ""
    StrCmp $1 "UBVRemux.ubv" 0 +2
      DeleteRegKey HKLM "Software\Classes\.ubv"
    DeleteRegKey HKLM "Software\Classes\UBVRemux.ubv"
    System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'
  assoc_done:

  ; Remove PATH entries if they were added during install
  ReadRegStr $0 HKLM "${UNINSTALL_REG_KEY}" "AddedToPath"
  StrCmp $0 "1" 0 +4
    EnVar::SetHKCU
    EnVar::DeleteValue "PATH" "$INSTDIR\gui"
    EnVar::DeleteValue "PATH" "$INSTDIR\cli"

  ; Remove Start Menu shortcuts
  Delete "$SMPROGRAMS\${PRODUCT_NAME}\UBV Remux.lnk"
  Delete "$SMPROGRAMS\${PRODUCT_NAME}\Uninstall.lnk"
  RMDir "$SMPROGRAMS\${PRODUCT_NAME}"

  ; Remove GUI files
  RMDir /r "$INSTDIR\gui"

  ; Remove CLI files
  RMDir /r "$INSTDIR\cli"

  ; Remove resource and root files
  RMDir /r "$INSTDIR\resource"
  Delete "$INSTDIR\Uninstall.exe"

  ; Remove install directory (only if empty)
  RMDir "$INSTDIR"

  ; Remove registry entries
  DeleteRegKey HKLM "${UNINSTALL_REG_KEY}"
SectionEnd
