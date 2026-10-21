#Requires AutoHotkey v2.0

Run("C:\Users\2supe\OneDrive\Desktop\ChatGPT.lnk")

; Wait for ChatGPT to open
WinWait("ahk_exe ChatGPT.exe")

; Move and resize the ChatGPT window
WinMove 60, 51, 1800, 975, "ahk_exe ChatGPT.exe"