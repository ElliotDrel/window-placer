# Window Placer

This initial commit captures the existing AutoHotkey window-placement implementation before migration to native Rust. The next version keeps this source as a historical snapshot.

The script waits after login, detects supported apps and launches app-specific scripts to size their windows. See `autohotkey/Auto Window Setup.ahk` and `_ResizeCommon.ahk` for the original implementation.
