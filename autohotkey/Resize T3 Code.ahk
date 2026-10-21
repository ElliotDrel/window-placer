#Requires AutoHotkey v2.0
#Include _ResizeCommon.ahk

; ============================================================
; Resize T3 Code - first window only, align to Akiflow's frame
; ============================================================
;
; 2026-09-15 ask: match the ChatGPT window's size and position exactly.
; (Supersedes the 2026-08-29 "mirror the gap to Akiflow's edges" target of
; x 125, y 53, w 1669, h 965.)
;
; ALIGN BY CLIENT RECT, NOT WINDOW RECT. ChatGPT has no invisible border;
; T3 Code carries a 9px inset on left/right/bottom (0 on top), so its window
; rect must overshoot by 9px per inset edge to land on the same client frame.
;   Shared client frame: L78 T53 R1841 B1009  (1763 x 956)
;   T3 Code rect:        L69 T53 R1850 B1018  -> x 69, y 53, w 1781, h 965
;
; Verify with AHK WinGetClientPos (Window Spy's "Client" row).
; DwmGetWindowAttribute reports this inset as 8, which is off by one.
; NOTE: the app's process is now "T3 Code (Alpha).exe" (was "(Nightly)");
; Auto Window Setup.ahk matches on that name.
;
; Only the window whose HWND is passed on the command line is touched (same
; pattern as "Resize Windows Terminal.ahk"). That HWND is chosen once by
; Auto Window Setup.ahk (the first T3 Code window seen this session) —
; every later window is left alone on purpose (2026-08-29 ask).
; ============================================================

args := ResizeArgs()
if (args.Length < 1)
    ExitApp

id := args[1] + 0

if !WinExist("ahk_id " id)
    ExitApp

; 2px tolerance by default; --force snaps to the exact target.
ResizeApply(id, 69, 53, 1781, 965)
