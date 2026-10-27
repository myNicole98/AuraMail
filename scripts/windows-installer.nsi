Unicode true
!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "FileFunc.nsh"
!include "WinVer.nsh"
!include "x64.nsh"

!ifndef APP_VERSION
  !define APP_VERSION "0.0.0"
!endif

!define APP_NAME "AuraMail"
!define APP_ID "dev.auramail.app"
!define APP_EXE "AuraMail.exe"
!define REG_KEY "Software\AuraMail"
!define UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_ID}"

Name "${APP_NAME} ${APP_VERSION}"
OutFile "../release/AuraMail-${APP_VERSION}-x64-setup.exe"
InstallDir "$LOCALAPPDATA\Programs\AuraMail"
InstallDirRegKey HKCU "${REG_KEY}" "InstallDir"
RequestExecutionLevel user
SetCompressor /SOLID lzma
CRCCheck on
XPStyle on
BrandingText "${APP_NAME} ${APP_VERSION}"

VIProductVersion "${APP_VERSION}.0"
VIAddVersionKey "ProductName" "${APP_NAME}"
VIAddVersionKey "ProductVersion" "${APP_VERSION}"
VIAddVersionKey "FileVersion" "${APP_VERSION}"
VIAddVersionKey "FileDescription" "${APP_NAME} installer"
VIAddVersionKey "CompanyName" "AuraMail"
VIAddVersionKey "LegalCopyright" "Copyright (C) AuraMail"

!define MUI_ICON "../src-tauri/icons/icon.ico"
!define MUI_UNICON "../src-tauri/icons/icon.ico"
!define MUI_ABORTWARNING
!define MUI_FINISHPAGE_RUN "$INSTDIR\${APP_EXE}"
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Var WebView2Missing

Function .onInit
  SetShellVarContext current
  ReadRegStr $0 HKLM "SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" "pv"
  ReadRegStr $1 HKCU "Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" "pv"
  ReadRegStr $2 HKLM "SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" "pv"
  StrCmp $0 "" 0 webview_ok
  StrCmp $1 "" 0 webview_ok
  StrCmp $2 "" 0 webview_ok
  StrCpy $WebView2Missing "1"
webview_ok:
FunctionEnd

Section "AuraMail" SecMain
  SetOutPath "$INSTDIR"
  File /oname=AuraMail.exe "../src-tauri/target/x86_64-pc-windows-msvc/release/auramail.exe"
  File /oname=auramail-proton-bridge.exe "../src-tauri/binaries/auramail-proton-bridge-x86_64-pc-windows-msvc.exe"

!ifdef WEBVIEW2_BOOTSTRAPPER
  File /oname=MicrosoftEdgeWebview2Setup.exe "${WEBVIEW2_BOOTSTRAPPER}"
!endif

  SetOutPath "$INSTDIR\resources\matugen"
  File "../resources/matugen/auramail.toml"
  File "../resources/matugen/auramail.json"
  SetOutPath "$INSTDIR\resources\logo"
  File "../resources/logo/aura_mail_icon_only.svg"
  SetOutPath "$INSTDIR"

  WriteUninstaller "$INSTDIR\Uninstall.exe"
  WriteRegStr HKCU "${REG_KEY}" "InstallDir" "$INSTDIR"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayName" "${APP_NAME}"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayVersion" "${APP_VERSION}"
  WriteRegStr HKCU "${UNINST_KEY}" "Publisher" "AuraMail"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayIcon" "$INSTDIR\${APP_EXE},0"
  WriteRegStr HKCU "${UNINST_KEY}" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoRepair" 1

  CreateDirectory "$SMPROGRAMS\AuraMail"
  CreateShortcut "$SMPROGRAMS\AuraMail\AuraMail.lnk" "$INSTDIR\${APP_EXE}" "" "$INSTDIR\${APP_EXE}" 0
  CreateShortcut "$DESKTOP\AuraMail.lnk" "$INSTDIR\${APP_EXE}" "" "$INSTDIR\${APP_EXE}" 0

  ${If} $WebView2Missing == "1"
    DetailPrint "Installing Microsoft Edge WebView2 Runtime..."
    IfFileExists "$INSTDIR\MicrosoftEdgeWebview2Setup.exe" 0 webview_open
      ExecWait '"$INSTDIR\MicrosoftEdgeWebview2Setup.exe" /silent /install' $0
      Goto webview_done
webview_open:
      StrCpy $0 "0"
      ExecShell "open" "https://go.microsoft.com/fwlink/p/?LinkId=2124703"
      MessageBox MB_ICONEXCLAMATION "AuraMail needs the Microsoft Edge WebView2 Runtime. Install it from Microsoft, then start AuraMail again."
webview_done:
    ${If} $0 != 0
      MessageBox MB_ICONEXCLAMATION "AuraMail needs the Microsoft Edge WebView2 Runtime. Automatic installation failed; install it from Microsoft and start AuraMail again."
    ${EndIf}
  ${EndIf}
  Delete "$INSTDIR\MicrosoftEdgeWebview2Setup.exe"
SectionEnd

Section "Uninstall"
  Delete "$INSTDIR\${APP_EXE}"
  Delete "$INSTDIR\auramail-proton-bridge.exe"
  Delete "$INSTDIR\MicrosoftEdgeWebview2Setup.exe"
  RMDir /r "$INSTDIR\resources"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"
  Delete "$SMPROGRAMS\AuraMail\AuraMail.lnk"
  RMDir "$SMPROGRAMS\AuraMail"
  Delete "$DESKTOP\AuraMail.lnk"
  DeleteRegKey HKCU "${UNINST_KEY}"
  DeleteRegKey HKCU "${REG_KEY}"
SectionEnd
