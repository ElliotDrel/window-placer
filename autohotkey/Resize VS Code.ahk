#Requires AutoHotkey v2.0
#Include _ResizeCommon.ahk

; Resize all VS Code windows to the specified position and size
ids := WinGetList("ahk_exe Code.exe")

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
    ResizeApply(id, 69, 56, 1782, 963)
}
