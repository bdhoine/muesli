; Muesli's additions to Tauri's NSIS installer (bundle > windows > nsis > installerHooks).
;
; "Delete the application data" in the uninstaller removes only the folders named after the
; bundle id. Muesli keeps its settings, state, meetings and vocabulary in %APPDATA%\Muesli and its
; downloaded models in %LOCALAPPDATA%\Muesli, so those go too; otherwise a reinstall skips
; Getting started. Transcripts (Documents\MeetingTranscripts by default) are the person's files
; and stay. Never during an update. Tauri itself removes the "Start when I sign in" Run value.

!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $DeleteAppDataCheckboxState = 1
  ${AndIf} $UpdateMode <> 1
    SetShellVarContext current
    RmDir /r "$APPDATA\Muesli"
    RmDir /r "$LOCALAPPDATA\Muesli"
  ${EndIf}
!macroend
