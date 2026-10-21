#Requires AutoHotkey v2.0
Persistent

; ============================================================
; Auto Window Setup
; 1) Wait 60 seconds after login to let startup apps settle.
; 2) Run one forced detection pass so already-open apps are handled.
; 3) Poll every second and handle only newly opened apps.
; ============================================================

startupDelayMs := 60000

; Initialize counts to 0 so the forced first pass can trigger
; resize scripts for apps that were launched before this script runs.
akiflowCount := 0
cursorCount  := 0
vscodeCount  := 0
granolaCount := 0
wisprCount   := 0
claudeCount  := 0
codexCount   := 0
slackCount   := 0
discordCount := 0
t3Count      := 0
terminalCount   := 0
terminalHandled := false  ; only the first Windows Terminal window ever gets resized
t3Handled       := false  ; only the first T3 Code window ever gets resized
codexHandled    := false  ; only the first ChatGPT window ever gets resized

; One-shot startup timer (negative interval = run once after delay).
SetTimer(StartMonitoring, -startupDelayMs)

StartMonitoring() {
    ; Forced first scan catches apps that are already open.
    CheckApps(true)

    ; Then continue normal polling for new launches.
    SetTimer(CheckApps, 1000)
}

; Count main windows for a given exe, applying the same filters as resize scripts.
; Skips tool windows, topmost (notifications), titleless, and owned (dialogs).
; Optional titleFilter requires the title to contain the given substring.
CountMainWindows(exe, titleFilter := "") {
    count := 0
    for id in WinGetList("ahk_exe " exe) {
        exStyle := WinGetExStyle(id)
        if (exStyle & 0x88)  ; WS_EX_TOOLWINDOW (0x80) or WS_EX_TOPMOST (0x08, notifications)
            continue
        title := WinGetTitle(id)
        if (title = "")
            continue
        if (DllCall("GetWindow", "Ptr", id, "UInt", 4, "Ptr"))  ; GW_OWNER = 4
            continue
        if (titleFilter != "" && !InStr(title, titleFilter))
            continue
        count++
    }
    return count
}

; Returns the HWND of the first main window matching the same filters as
; CountMainWindows, or 0 if none. Used to pin a resize to one specific
; window (e.g. only the first Windows Terminal window ever opened).
GetFirstMainWindow(exe, titleFilter := "") {
    for id in WinGetList("ahk_exe " exe) {
        exStyle := WinGetExStyle(id)
        if (exStyle & 0x88)
            continue
        title := WinGetTitle(id)
        if (title = "")
            continue
        if (DllCall("GetWindow", "Ptr", id, "UInt", 4, "Ptr"))
            continue
        if (titleFilter != "" && !InStr(title, titleFilter))
            continue
        return id
    }
    return 0
}

; force = true  -> trigger resize for currently open apps (startup pass)
; force = false -> trigger resize only when main window count increases
CheckApps(force := false) {
    global akiflowCount, cursorCount, vscodeCount, granolaCount, wisprCount, claudeCount, codexCount, slackCount
    global terminalCount, terminalHandled, t3Count, t3Handled, codexHandled, discordCount

    akiflowCountNow  := CountMainWindows("Akiflow.exe")
    cursorCountNow   := CountMainWindows("Cursor.exe")
    vscodeCountNow   := CountMainWindows("Code.exe")
    granolaCountNow  := CountMainWindows("Granola.exe")
    wisprCountNow    := CountMainWindows("Wispr Flow.exe", "Wispr Flow")
    claudeCountNow   := CountMainWindows("Claude.exe")
    codexCountNow    := CountMainWindows("ChatGPT.exe")
    slackCountNow    := CountMainWindows("slack.exe")
    discordCountNow  := CountMainWindows("Discord.exe")
    terminalCountNow := CountMainWindows("WindowsTerminal.exe")
    t3CountNow       := CountMainWindows("T3 Code (Alpha).exe")

    ; Keep the 3s delay so target windows have time to fully initialize.
    if (akiflowCountNow > 0 && (force || akiflowCountNow > akiflowCount))
        SetTimer(() => Run(A_ScriptDir "\Resize Akiflow.ahk"), -3000)

    if (cursorCountNow > 0 && (force || cursorCountNow > cursorCount))
        SetTimer(() => Run(A_ScriptDir "\Resize Cursor.ahk"), -3000)

    if (vscodeCountNow > 0 && (force || vscodeCountNow > vscodeCount))
        SetTimer(() => Run(A_ScriptDir "\Resize VS Code.ahk"), -3000)

    if (granolaCountNow > 0 && (force || granolaCountNow > granolaCount))
        SetTimer(() => Run(A_ScriptDir "\Resize Granola.ahk"), -3000)

    if (wisprCountNow > 0 && (force || wisprCountNow > wisprCount))
        SetTimer(() => Run(A_ScriptDir "\Resize Wispr Flow.ahk"), -3000)

    if (claudeCountNow > 0 && (force || claudeCountNow > claudeCount))
        SetTimer(() => Run(A_ScriptDir "\Resize Claude.ahk"), -3000)

    if (slackCountNow > 0 && (force || slackCountNow > slackCount))
        SetTimer(() => Run(A_ScriptDir "\Resize Slack.ahk"), -3000)

    if (discordCountNow > 0 && (force || discordCountNow > discordCount))
        SetTimer(() => Run(A_ScriptDir "\Resize Discord.ahk"), -3000)

    ; ChatGPT: resize only the first window of a batch. Keep the chosen HWND
    ; pinned so new ChatGPT windows stay at their own size until every
    ; ChatGPT window has closed.
    if (codexCountNow = 0)
        codexHandled := false

    if (!codexHandled && codexCountNow > 0 && (force || codexCountNow > codexCount)) {
        codexTargetId := GetFirstMainWindow("ChatGPT.exe")
        if (codexTargetId) {
            codexHandled := true
            SetTimer(() => Run(A_ScriptDir "\Resize Codex-ChatGPT.ahk " . codexTargetId), -3000)
        }
    }

    ; Windows Terminal: only ever resize the first window of a "batch" -
    ; once terminalHandled flips true it stays latched (ignoring every
    ; further window) until the count drops back to zero, at which point
    ; it resets so the next window to open is treated as first again.
    if (terminalCountNow = 0)
        terminalHandled := false

    if (!terminalHandled && terminalCountNow > 0 && (force || terminalCountNow > terminalCount)) {
        targetId := GetFirstMainWindow("WindowsTerminal.exe")
        if (targetId) {
            terminalHandled := true
            SetTimer(() => Run(A_ScriptDir "\Resize Windows Terminal.ahk " . targetId), -3000)
        }
    }

    ; T3 Code: same first-window-only latch as Windows Terminal above -
    ; only the first T3 Code window ever seen gets resized, and the latch
    ; resets once the count drops back to zero.
    if (t3CountNow = 0)
        t3Handled := false

    if (!t3Handled && t3CountNow > 0 && (force || t3CountNow > t3Count)) {
        t3TargetId := GetFirstMainWindow("T3 Code (Alpha).exe")
        if (t3TargetId) {
            t3Handled := true
            SetTimer(() => Run(A_ScriptDir "\Resize T3 Code.ahk " . t3TargetId), -3000)
        }
    }

    ; Update baseline state for the next poll.
    akiflowCount  := akiflowCountNow
    cursorCount   := cursorCountNow
    vscodeCount   := vscodeCountNow
    granolaCount  := granolaCountNow
    wisprCount    := wisprCountNow
    claudeCount   := claudeCountNow
    codexCount    := codexCountNow
    slackCount    := slackCountNow
    discordCount  := discordCountNow
    terminalCount := terminalCountNow
    t3Count       := t3CountNow
}
