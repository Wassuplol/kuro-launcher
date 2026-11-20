; Roblox's own launch schemes, given back when this app is uninstalled.
;
; A scheme pointing at a deleted executable is a game link that opens nothing, and the
; person who removes a launcher still expects Roblox's own to work. The value this app
; displaced lives in HKCU\Software\Kuro Launcher\Schemes\<scheme> for exactly this moment:
; the data folder is already gone by the time an uninstaller runs.
;
; Three cases, and the third is the one that matters most:
;
; - a remembered command: write it back verbatim;
; - the marker `-`: this app took a scheme nobody owned, so the honest outcome is no scheme;
; - no remembered value at all: this app never touched the scheme, and an uninstall must not
;   touch it either. Deleting Roblox's own handler here would be the worst bug in the file.

!macro KURO_RESTORE_SCHEME SCHEME
  ClearErrors
  ReadRegStr $0 HKCU "Software\Kuro Launcher\Schemes\${SCHEME}" "previous"
  ${If} $0 == "-"
    DeleteRegKey HKCU "Software\Classes\${SCHEME}"
  ${ElseIf} $0 != ""
    WriteRegStr HKCU "Software\Classes\${SCHEME}" "" "URL:Roblox launch link"
    WriteRegStr HKCU "Software\Classes\${SCHEME}" "URL Protocol" ""
    WriteRegStr HKCU "Software\Classes\${SCHEME}\shell\open\command" "" "$0"
  ${EndIf}
  DeleteRegKey HKCU "Software\Kuro Launcher\Schemes\${SCHEME}"
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro KURO_RESTORE_SCHEME "roblox-player"
  !insertmacro KURO_RESTORE_SCHEME "roblox"
!macroend
