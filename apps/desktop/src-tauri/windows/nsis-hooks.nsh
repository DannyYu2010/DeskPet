; DeskPet Windows 11 installer policy.
;
; Program files, Start Menu shortcuts and uninstall registry entries are owned
; by Tauri's NSIS template and are removed by its standard uninstaller.
;
; User content deliberately lives outside $INSTDIR at:
;   $APPDATA\app.deskpet\packs
; The standard uninstaller does not touch $APPDATA, so imported pet packs and
; the active-pack configuration survive uninstall/reinstall. Do not add an
; RMDir for that directory here.

!macro NSIS_HOOK_PREINSTALL
  ; Windows 11 reports NT 10.0 with build 22000 or newer. Reading the build
  ; number avoids accepting Windows 10 merely because both share major 10.
  ReadRegStr $0 HKLM "SOFTWARE\Microsoft\Windows NT\CurrentVersion" "CurrentBuildNumber"
  ${If} $0 == ""
    MessageBox MB_ICONSTOP|MB_OK "DeskPet could not determine the Windows version. Windows 11 is required."
    Abort
  ${EndIf}

  IntCmpU $0 22000 deskpet_win11_ok deskpet_win11_unsupported deskpet_win11_ok

  deskpet_win11_unsupported:
    MessageBox MB_ICONSTOP|MB_OK "DeskPet requires Windows 11 (build 22000 or newer)."
    Abort

  deskpet_win11_ok:
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ; A fresh Windows install starts with the bundled generic pack. Never
  ; overwrite an existing config: reinstalling must preserve the user's active
  ; pack, sleep time and window placement.
  IfFileExists "$INSTDIR\packs\default\manifest.json" 0 deskpet_config_done
  CreateDirectory "$APPDATA\app.deskpet"
  IfFileExists "$APPDATA\app.deskpet\config.toml" deskpet_config_done 0
  CopyFiles /SILENT "$INSTDIR\windows\default-config.toml" "$APPDATA\app.deskpet\config.toml"
  deskpet_config_done:
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; Intentionally empty. Tauri removes the application, shortcuts and
  ; uninstall registration. User packs under $APPDATA remain untouched.
!macroend
