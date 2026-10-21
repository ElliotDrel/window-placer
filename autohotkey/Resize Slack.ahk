#Requires AutoHotkey v2.0
#Include _ResizeCommon.ahk

; ============================================================
; Resize Slack window
; ============================================================
;
; Original position (measured): x=143, y=53, w=1625, h=982
;   Screen: 1920 x 1080, work area right=1920 (no horizontal taskbar offset)
;   Left margin = 143, right margin = 1920 - (143+1625) = 152 -> not centered (9px off)
;
; Math:
;   Centered X = (1920 - 1625) / 2 = 147.5 -> rounded to 148
;   Y unchanged at 53 (already matches Akiflow's top edge exactly)
;   Width unchanged at 1625 (per Elliot: don't change width)
;   Height = 966, so bottom = 53 + 966 = 1019, matching Cursor/VS Code's bottom
;   (Cursor & VS Code: y=56,h=963 -> bottom=1019. Trying this on Slack per Elliot's ask.)
; ============================================================

ids := WinGetList("ahk_exe slack.exe")

for id in ids {
    ; Skip tool windows, hidden windows, and windows without a title
    exStyle := WinGetExStyle(id)
    if (exStyle & 0x88)  ; WS_EX_TOOLWINDOW (0x80) or WS_EX_TOPMOST (0x08, notifications)
        continue
    title := WinGetTitle(id)
    if (title = "")
        continue
    ; Skip dialogs and popups (owned windows)
    if (DllCall("GetWindow", "Ptr", id, "UInt", 4, "Ptr"))  ; GW_OWNER = 4
        continue
    ; 2px tolerance by default; --force snaps to the exact target.
    ResizeApply(id, 148, 53, 1625, 966)
}
