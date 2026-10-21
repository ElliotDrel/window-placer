#Requires AutoHotkey v2.0
#Include _ResizeCommon.ahk

; ============================================================
; Setup Wispr Flow - Move and resize window to align with Akiflow
; ============================================================
;
; Original Wispr Flow window (from Window Spy):
;   Position: x: 115, y: 57
;   Size:     w: 1690, h: 951
;   Bottom edge was at: y + h = 57 + 951 = 1008
;
; New position aligns top to Akiflow, bottom kept at original:
;   Akiflow top: y = 53
;   Original bottom: 57 + 951 = 1008 (preserved)
;
; Math:
;   New Y = 53 (aligned to Akiflow top)
;   New H = 1008 - 53 = 955 (original bottom preserved)
;   X unchanged at 115
;   Width unchanged at 1690
; ============================================================

ids := WinGetList("ahk_exe Wispr Flow.exe")

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
    ; Only target the main window (title was "Hub" prior to a Wispr Flow update; now "Wispr Flow")
    if !InStr(title, "Wispr Flow")
        continue
    ; 2px tolerance by default; --force snaps to the exact target.
    ResizeApply(id, 115, 53, 1690, 955)
}
