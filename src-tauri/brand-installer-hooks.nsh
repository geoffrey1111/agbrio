; ProductName changes NSIS registration keys. Migrate only a proven same-directory
; legacy install; never remove user data or override a chosen new directory.
!ifndef AGBRIO_LEGACY_UNINSTALL
!define AGBRIO_LEGACY_UNINSTALL "Software\Microsoft\Windows\CurrentVersion\Uninstall\AI Work Router"
!endif
!ifndef AGBRIO_LEGACY_PRODUCT
!define AGBRIO_LEGACY_PRODUCT "${MANUKEY}\AI Work Router"
!endif
Var AgbrioLegacyDirectory
Var AgbrioLegacyMigration

; Parameterized directory keeps shortcut operations shared by the real hooks
; and isolated installation validation; it introduces no installer test switch.
!macro AGBRIO_INSTALL_SHARED_ENTRY programsDirectory installDirectory
  CreateDirectory "${programsDirectory}"
  CreateShortCut "${programsDirectory}\Codex (Agbrio shared).lnk" "$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" '-NoProfile -WindowStyle Hidden -ExecutionPolicy Bypass -File "${installDirectory}\codex-shared\launch.ps1"' "${installDirectory}\${MAINBINARYNAME}.exe"
!macroend

!macro AGBRIO_REMOVE_SHARED_ENTRY programsDirectory installDirectory
  ; The PowerShell target is shared by many shortcuts. Verify the exact launcher
  ; arguments too, so removing an old installation preserves a replaced entry.
  Push $0
  Push $1
  Push $2
  Push $3
  !insertmacro ComHlpr_CreateInProcInstance ${CLSID_ShellLink} ${IID_IShellLink} r0 ""
  ${If} $0 P<> 0
    ${IUnknown::QueryInterface} $0 '("${IID_IPersistFile}", .r1)'
    ${If} $1 P<> 0
      ${IPersistFile::Load} $1 '("${programsDirectory}\Codex (Agbrio shared).lnk", ${STGM_READ})'
      StrCpy $2 ""
      StrCpy $3 ""
      ${IShellLink::GetArguments} $0 '(.r2, ${NSIS_MAX_STRLEN})'
      ${IShellLink::GetPath} $0 '(.r3, ${NSIS_MAX_STRLEN}, 0, ${SLGP_RAWPATH})'
      ${If} $2 == '-NoProfile -WindowStyle Hidden -ExecutionPolicy Bypass -File "${installDirectory}\codex-shared\launch.ps1"'
      ${AndIf} $3 == "$SYSDIR\WindowsPowerShell\v1.0\powershell.exe"
        Delete "${programsDirectory}\Codex (Agbrio shared).lnk"
      ${EndIf}
      ${IUnknown::Release} $1 ""
    ${EndIf}
    ${IUnknown::Release} $0 ""
  ${EndIf}
  Pop $3
  Pop $2
  Pop $1
  Pop $0
!macroend

!macro NSIS_HOOK_PREINSTALL
  StrCpy $AgbrioLegacyMigration 0
  ReadRegStr $AgbrioLegacyDirectory SHCTX "${AGBRIO_LEGACY_PRODUCT}" ""
  ReadRegStr $R0 SHCTX "${MANUPRODUCTKEY}" ""
  ${If} $R0 == ""
  ${AndIf} $AgbrioLegacyDirectory != ""
    IfFileExists "$AgbrioLegacyDirectory\${MAINBINARYNAME}.exe" 0 agbrio_pre_done
    ${If} $INSTDIR == "$LOCALAPPDATA\${PRODUCTNAME}"
      StrCpy $INSTDIR $AgbrioLegacyDirectory
    ${EndIf}
    ${If} $INSTDIR == $AgbrioLegacyDirectory
      StrCpy $AgbrioLegacyMigration 1
    ${EndIf}
  ${EndIf}
  agbrio_pre_done:
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro AGBRIO_REMOVE_SHARED_ENTRY "$SMPROGRAMS\Agbrio" "$INSTDIR"
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ; Separate opt-in entry. Never replace the official Codex shortcut/defaults.
  !insertmacro AGBRIO_INSTALL_SHARED_ENTRY "$SMPROGRAMS\Agbrio" "$INSTDIR"
  ${If} $AgbrioLegacyMigration == 1
    ReadRegStr $R0 SHCTX "${MANUPRODUCTKEY}" ""
    ${If} $R0 == $AgbrioLegacyDirectory
      DeleteRegKey SHCTX "${AGBRIO_LEGACY_UNINSTALL}"
      DeleteRegKey SHCTX "${AGBRIO_LEGACY_PRODUCT}"
      !insertmacro IsShortcutTarget "$SMPROGRAMS\AI Work Router\AI Work Router.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
      Pop $R0
      ${If} $R0 == 1
        Delete "$SMPROGRAMS\AI Work Router\AI Work Router.lnk"
      ${EndIf}
      !insertmacro IsShortcutTarget "$DESKTOP\AI Work Router.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
      Pop $R0
      ${If} $R0 == 1
        Delete "$DESKTOP\AI Work Router.lnk"
      ${EndIf}
    ${EndIf}
  ${EndIf}
!macroend
