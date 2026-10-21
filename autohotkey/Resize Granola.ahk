#Requires AutoHotkey v2.0
#Include _ResizeCommon.ahk

; ============================================================
; Setup Granola - Move and resize window to align with Akiflow
; ============================================================
;
; Original Granola window (from Window Spy):
;   Position: x: 210, y: 55
;   Size:     w: 1500, h: 965
;   Bottom edge was at: y + h = 55 + 965 = 1020
;
; New position aligns top and bottom to match Akiflow after resize:
;   Akiflow top: y = 53
;   Akiflow bottom: 53 + 967 = 1020
;
; Math:
;   New Y = 53 (aligned to Akiflow top)
;   New H = 1020 - 53 = 967 (flush with Akiflow bottom / taskbar)
;   X unchanged at 210
;   Width unchanged at 1500
; ============================================================

ids := WinGetList("ahk_exe Granola.exe")

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
    ResizeApply(id, 210, 53, 1500, 967)
}
