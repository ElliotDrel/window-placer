#Requires AutoHotkey v2.0
#Include _ResizeCommon.ahk

; ============================================================
; Force Resize All - snap every window to its exact target
; ============================================================
;
; Double-click this (or run it from a shell) to re-apply every "Resize *.ahk"
; target with --force, bypassing their 2px tolerance. Use it after changing a
; target value: a 1-2px correction falls inside the tolerance and would
; otherwise silently never apply.
;
; Normal startup placement still goes through Auto Window Setup.ahk, which
; runs these scripts WITHOUT --force so the tolerance keeps doing its job.
;
; Apps that aren't running are skipped.
; ============================================================

; Scripts that find and resize every matching window themselves.
batchScripts := ["Resize Akiflow.ahk"
               , "Resize Claude.ahk"
               , "Resize Discord.ahk"
               , "Resize Cursor.ahk"
               , "Resize VS Code.ahk"
               , "Resize Granola.ahk"
               , "Resize Slack.ahk"
               , "Resize Wispr Flow.ahk"]

; Scripts that take the HWND of one specific window: script -> [exe, titleFilter].
hwndScripts := Map(
    "Resize Codex-ChatGPT.ahk",    ["ChatGPT.exe", ""],
    "Resize T3 Code.ahk",          ["T3 Code (Alpha).exe", ""],
    "Resize Windows Terminal.ahk", ["WindowsTerminal.exe", ""])

for name in batchScripts
    RunWait(Format('"{1}" "{2}\{3}" --force', A_AhkPath, A_ScriptDir, name))

for name, spec in hwndScripts {
    id := ResizeFirstMainWindow(spec[1], spec[2])
    if id
        RunWait(Format('"{1}" "{2}\{3}" {4} --force', A_AhkPath, A_ScriptDir, name, id))
}

ExitApp
