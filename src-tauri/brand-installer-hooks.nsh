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

!macro NSIS_HOOK_POSTINSTALL
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
