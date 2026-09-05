; MakrStudio NSIS installer hooks — NDI Redistributable silent install
; Runs as part of MakrStudio's own NSIS install, mirroring how WebView2 is handled
; via embedBootstrapper (tauri.conf.json bundle.windows.webviewInstallMode).
; See src-tauri/resources/NDI_VERSION.txt for version/currency notes.
; Docs: https://docs.ndi.video/all/developing-with-ndi/sdk/software-distribution
; May use /verysilent per NDI license, if our EULA covers NDI terms (our EULA does).

!include "LogicLib.nsh"

; Post-install runs after all files, registry keys and shortcuts are created.
; $INSTDIR is the MakrStudio install directory (e.g. C:\Program Files\MakrStudio).
; The NDI redistributable is bundled as a resource and available at
; $INSTDIR\NDI_Runtime_V6.exe (mapped via bundle.resources) or
; $INSTDIR\resources\NDI_Runtime_V6.exe — we check both.

!macro NSIS_HOOK_POSTINSTALL
  DetailPrint "MakrStudio: Checking NDI Runtime..."

  ; Check if NDI already installed via registry (both 64-bit and WOW6432Node)
  ReadRegStr $0 HKLM "SOFTWARE\NDI\Runtime" "InstallDir"
  ${If} $0 != ""
    DetailPrint "NDI Runtime already installed at $0 (HKLM\SOFTWARE\NDI\Runtime), skipping redistributable"
    Goto ndi_done
  ${EndIf}

  ReadRegStr $0 HKLM "SOFTWARE\WOW6432Node\NDI\Runtime" "InstallDir"
  ${If} $0 != ""
    DetailPrint "NDI Runtime already installed at $0 (WOW6432Node), skipping"
    Goto ndi_done
  ${EndIf}

  ; Also check the runtime env var location (set by previous redistributable install)
  ; Env var is set system-wide, but NSIS may not see it until after reboot — registry check above is primary.
  ; If not installed, run the bundled redistributable silently.
  ; Locate the bundled installer (try both possible resource locations)
  StrCpy $1 "$INSTDIR\NDI_Runtime_V6.exe"
  IfFileExists "$1" ndi_found
    StrCpy $1 "$INSTDIR\resources\NDI_Runtime_V6.exe"
    IfFileExists "$1" ndi_found
      DetailPrint "NDI redistributable not found in $INSTDIR (expected NDI_Runtime_V6.exe), skipping — will fallback to manual SDK check at runtime"
      Goto ndi_done
    ndi_found:

  DetailPrint "Installing NDI Runtime via $1 /verysilent (NDI redistributable, ~9 MB)..."
  ; Run silently, wait for completion. /verysilent is Inno Setup silent (no UI).
  ; The NDI redistributable also supports /SILENT and /VERYSILENT — we use /verysilent per NDI docs.
  ExecWait '"$1" /verysilent' $2
  DetailPrint "NDI Runtime installer exit code: $2"
  ${If} $2 == 0
    DetailPrint "NDI Runtime installed successfully"
  ${Else}
    DetailPrint "NDI Runtime installer returned non-zero ($2) — may require reboot or manual install; see README.md/WINDOWS.md"
  ${EndIf}

  ndi_done:
!macroend

; No pre-install or uninstall hooks needed for NDI. Uninstall leaves NDI runtime
; in place (shared with other NDI apps like OBS/DistoAV), mirroring WebView2 behavior.

!macro NSIS_HOOK_PREINSTALL
!macroend

!macro NSIS_HOOK_PREUNINSTALL
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
!macroend
