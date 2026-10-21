#Requires AutoHotkey v2.0
#Include _ResizeCommon.ahk

; ALIGN THESE WINDOWS BY CLIENT RECT, NOT WINDOW RECT.
; ChatGPT is the one app here with no invisible border; Akiflow, Claude,
; Discord and T3 Code all carry a 9px inset on left/right/bottom (0 on top),
; so their window rect is 9px bigger per inset edge than the content you see.
; Measured 2026-09-15 with AHK WinGetPos vs WinGetClientPos:
;   Akiflow rect L23 T53 R1896 B1018 -> client L32 T53 R1887 B1009 (inset 9)
;   ChatGPT rect = client exactly                                  (inset 0)
; DwmGetWindowAttribute reports the inset as 8, which is off by one - trust
; WinGetClientPos, which is what Window Spy's "Client" row shows.
;
; Shared client frame for ChatGPT + T3 Code + Discord + Claude:
;   L78 T53 R1841 B1009  (1763 x 956)
; Bottom 1009 is Akiflow's client bottom (the master, per Elliot's ask).
; Horizontal (2026-09-15 ask): client gap was 125 - 32 = 93, half rounds up
;   to 47, so left edge moved out to 125 - 47 = 78 and the right edge moved
;   out the same 47 from 1794 -> 1841. Width = 1841 - 78 = 1763.
;
; ChatGPT has zero inset, so its window rect IS that client frame.
chatGptX := 78
chatGptY := 53
chatGptWidth := 1763
chatGptHeight := 956

; Auto Window Setup passes the exact HWND of the first eligible ChatGPT
; window. Later ChatGPT windows are never passed here.
args := ResizeArgs()
if (args.Length < 1)
    ExitApp

id := args[1]
target := "ahk_id " id
if !WinExist(target)
    ExitApp
if (WinGetProcessName(target) != "ChatGPT.exe")
    ExitApp

; Reject tool windows, notifications, titleless windows, and owned dialogs.
exStyle := WinGetExStyle(target)
if (exStyle & 0x88)
    ExitApp
if (WinGetTitle(target) = "")
    ExitApp
if DllCall("GetWindow", "Ptr", id, "UInt", 4, "Ptr")
    ExitApp

; 2px tolerance by default; --force snaps to the exact target.
ResizeApply(target, chatGptX, chatGptY, chatGptWidth, chatGptHeight)
