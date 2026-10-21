#Requires AutoHotkey v2.0
#Include _ResizeCommon.ahk

; ============================================================
; Resize Windows Terminal - first window only, on startup
; ============================================================
;
; Top/left/right match Akiflow exactly (Resize Akiflow.ahk: x=23, y=53,
; w=1873). Bottom is pinned to where Elliot had it manually positioned
; (measured live via AHK WinGetPos on 2026-08-29: x=24,y=54,w=1870,h=908,
; bottom=962), not to Akiflow's bottom. Height = 962 - 53 = 909.
;
; Only the window whose HWND is passed on the command line is touched. That
; HWND is chosen once by Auto Window Setup.ahk (the first Windows Terminal
; window seen this session) — every later window is left alone on purpose.
; ============================================================

args := ResizeArgs()
if (args.Length < 1)
    ExitApp

id := args[1] + 0

if !WinExist("ahk_id " id)
    ExitApp

; 2px tolerance by default; --force snaps to the exact target.
ResizeApply(id, 23, 53, 1873, 909)
