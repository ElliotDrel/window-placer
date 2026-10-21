#Requires AutoHotkey v2.0
#Include _ResizeCommon.ahk

; ============================================================
; Setup Akiflow - Move and resize window to fit above taskbar
; ============================================================
;
; Original Akiflow window (from Window Spy):
;   Position: x: 23, y: 25
;   Size:     w: 1873, h: 969
;   Bottom edge was at: y + h = 25 + 969 = 994
;
; Screen: 1080p (1920 x 1080)
; Taskbar top derived from Granola (confirmed flush): y: 55, h: 965 -> 55 + 965 = 1020
; Taskbar height: 1080 - 1020 = 60px
;
; Math:
;   New Y = 53 (manually tuned to perfectly touch taskbar)
;   Shift = 53 - 25 = 28 pixels down from original position
;   X unchanged at 23 (no horizontal movement)
;   Width unchanged at 1873
;   Height = 965 -> bottom = 53 + 965 = 1018 (nudged 2px up off the taskbar per Elliot's ask)
; ============================================================

ids := WinGetList("ahk_exe Akiflow.exe")

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
    ResizeApply(id, 23, 53, 1873, 965)
}
