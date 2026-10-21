#Requires AutoHotkey v2.0
#Include _ResizeCommon.ahk

; ============================================================
; Resize Discord - align bottom edge to Akiflow's bottom
; ============================================================
;
; 2026-09-15 ask: match the ChatGPT window's size and position exactly.
;
; ALIGN BY CLIENT RECT, NOT WINDOW RECT. ChatGPT has no invisible border;
; Discord carries a 9px inset on left/right/bottom (0 on top), so its window
; rect must overshoot by 9px per inset edge to land on the same client frame.
;   Shared client frame: L78 T53 R1841 B1009  (1763 x 956)
;   Discord rect:        L69 T53 R1850 B1018  -> x 69, y 53, w 1781, h 965
;
; Verify with AHK WinGetClientPos (Window Spy's "Client" row).
; DwmGetWindowAttribute reports this inset as 8, which is off by one.
; ============================================================

ids := WinGetList("ahk_exe Discord.exe")

for id in ids {
    ; Skip tool windows, hidden windows, and windows without a title
    exStyle := WinGetExStyle(id)
    if (exStyle & 0x88)  ; WS_EX_TOOLWINDOW (0x80) or WS_EX_TOPMOST (0x08, notifications)
        continue
    title := WinGetTitle(id)
    if (title = "")
        continue
    ; Skip dialogs and popups (owned windows like "Open Folder", etc.)
    if (DllCall("GetWindow", "Ptr", id, "UInt", 4, "Ptr"))  ; GW_OWNER = 4
        continue
    ; 2px tolerance by default; --force snaps to the exact target.
    ResizeApply(id, 69, 53, 1781, 965)
}
