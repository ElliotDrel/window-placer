#Requires AutoHotkey v2.0

; ============================================================
; Shared helpers for the "Resize *.ahk" scripts
; ============================================================
;
; Default behaviour: a window already within 2px of its target on every edge
; is left alone. That tolerance stops needless WinMove churn when the poller
; re-fires during a display/DWM change.
;
; Pass --force on the command line to bypass the tolerance and snap the
; window to the exact target. Use it after changing a target value, because
; a 1-2px correction otherwise falls inside the tolerance and silently never
; applies. "Force Resize All.ahk" runs every resize script this way.
; ============================================================

; True when --force was passed.
ResizeForced() {
    for arg in A_Args
        if (arg = "--force")
            return true
    return false
}

; Command-line args with flags stripped, so scripts that expect an HWND can
; find it no matter where --force sits on the line.
ResizeArgs() {
    out := []
    for arg in A_Args
        if (SubStr(arg, 1, 1) != "-")
            out.Push(arg)
    return out
}

; Protect maximized windows and windows covering their entire monitor.
; MonitorFromWindow uses the window's own monitor, including secondary displays.
ResizeIsProtected(target) {
    if (WinGetMinMax(target) = 1)
        return true
    id := WinExist(target)
    monitor := DllCall("MonitorFromWindow", "Ptr", id, "UInt", 2, "Ptr")
    info := Buffer(40, 0)
    NumPut("UInt", 40, info)
    if !DllCall("GetMonitorInfoW", "Ptr", monitor, "Ptr", info)
        return true  ; Leave the window alone if its monitor cannot be checked.
    WinGetPos &x, &y, &w, &h, target
    return (x <= NumGet(info, 4, "Int") && y <= NumGet(info, 8, "Int")
        && x + w >= NumGet(info, 12, "Int") && y + h >= NumGet(info, 16, "Int"))
}

; Move a window to the target, honouring the 2px tolerance unless --force.
; Fullscreen/maximized protection always applies, including with --force.
; Returns true if the window was actually moved.
ResizeApply(target, tx, ty, tw, th) {
    if ResizeIsProtected(target)
        return false
    if !ResizeForced() {
        WinGetPos &cx, &cy, &cw, &ch, target
        if (Abs(cx - tx) <= 2 && Abs(cy - ty) <= 2 && Abs(cw - tw) <= 2 && Abs(ch - th) <= 2)
            return false
    }
    WinMove tx, ty, tw, th, target
    return true
}

; True when a window is a real main window: not a tool window, not a
; notification, not titleless, not an owned dialog. Same filters the resize
; scripts and Auto Window Setup.ahk apply.
ResizeIsMainWindow(id, titleFilter := "") {
    if (WinGetExStyle(id) & 0x88)  ; WS_EX_TOOLWINDOW | WS_EX_TOPMOST
        return false
    title := WinGetTitle(id)
    if (title = "")
        return false
    if (DllCall("GetWindow", "Ptr", id, "UInt", 4, "Ptr"))  ; GW_OWNER
        return false
    if (titleFilter != "" && !InStr(title, titleFilter))
        return false
    return true
}

; HWND of the first main window for an exe, or 0. Used by the force-all
; runner to feed the scripts that resize one specific window.
ResizeFirstMainWindow(exe, titleFilter := "") {
    for id in WinGetList("ahk_exe " exe)
        if ResizeIsMainWindow(id, titleFilter)
            return id
    return 0
}
