//! Thin safe wrappers over the Win32 calls we need. All `unsafe` lives here.
//!
//! Shared invariants for the `unsafe` blocks below:
//! * Window handles are plain values passed to user32, which validates them;
//!   a stale or recycled HWND makes calls fail or describe another window. It
//!   never causes memory unsafety, and identity is rechecked by the caller
//!   (`tracker::Tracker::on_timer` compares the owner PID right before moving).
//! * Every out-pointer refers to a live, correctly sized local for the duration
//!   of the call, and sizes passed in match the buffers.
//! * Owned kernel/user objects (process handle, mutex, hooks) are released
//!   exactly once, either right away or by a `Drop` impl.
//! * Hook and enum callbacks run on the single program thread (out-of-context
//!   WinEvents are delivered inside `GetMessageW`), so thread-locals are safe;
//!   the callbacks only push to a queue and never re-enter program logic.

use std::cell::{Cell, RefCell};
use std::ptr::null_mut;
use std::sync::atomic::{AtomicU32, Ordering};

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_ACCESS_DENIED, ERROR_ALREADY_EXISTS, ERROR_INVALID_WINDOW_HANDLE,
    GetLastError, HANDLE, HWND, LPARAM, RECT,
};
use windows_sys::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
use windows_sys::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
};
use windows_sys::Win32::System::Console::{
    ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_OUTPUT_HANDLE, SetConsoleCtrlHandler,
};
use windows_sys::Win32::System::Threading::{
    CreateMutexW, GetCurrentThreadId, OpenProcess, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows_sys::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows_sys::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CHILDID_SELF, DispatchMessageW, EVENT_OBJECT_DESTROY, EVENT_OBJECT_NAMECHANGE,
    EVENT_OBJECT_SHOW, EnumWindows, GW_OWNER, GWL_EXSTYLE, GWL_STYLE, GetMessageW, GetWindow,
    GetWindowLongPtrW, GetWindowRect, GetWindowTextLengthW, GetWindowTextW,
    GetWindowThreadProcessId, IsIconic, IsWindowVisible, IsZoomed, KillTimer, MSG, OBJID_WINDOW,
    PM_NOREMOVE, PeekMessageW, PostThreadMessageW, SWP_ASYNCWINDOWPOS, SWP_NOACTIVATE,
    SWP_NOOWNERZORDER, SWP_NOZORDER, SetTimer, SetWindowPos, WINEVENT_OUTOFCONTEXT,
    WINEVENT_SKIPOWNPROCESS, WM_APP, WM_QUIT, WM_TIMER, WS_CHILD, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
};

use crate::geometry::{ScreenRect, target_rect};
use crate::rules::{PlacementRect, Rule, exe_file_name, title_matches};
use crate::tracker::{PlaceOutcome, Readiness};

/// A top-level window handle as a plain integer (HWND value).
pub type WindowId = isize;

fn hwnd(id: WindowId) -> HWND {
    id as HWND
}

impl From<RECT> for ScreenRect {
    fn from(r: RECT) -> Self {
        ScreenRect {
            left: r.left,
            top: r.top,
            right: r.right,
            bottom: r.bottom,
        }
    }
}

/// Make all coordinates physical pixels. Fails harmlessly if already set.
pub fn enable_per_monitor_dpi() {
    // SAFETY: takes a predefined pseudo-handle constant; no pointers.
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}

/// For a GUI-subsystem build launched from a terminal: borrow the parent's
/// console so diagnostics are visible. No-op when stdout is already usable.
pub fn attach_parent_console() {
    // SAFETY: no pointers; failure (no parent console) is ignored.
    unsafe {
        if GetStdHandle(STD_OUTPUT_HANDLE).is_null() {
            AttachConsole(ATTACH_PARENT_PROCESS);
        }
    }
}

// ---------------------------------------------------------------------------
// Window inspection

/// Owning process ID, or `None` if the window no longer exists.
pub fn window_pid(id: WindowId) -> Option<u32> {
    let mut pid = 0u32;
    // SAFETY: `pid` is a live local; invalid HWNDs make the call return 0.
    let thread = unsafe { GetWindowThreadProcessId(hwnd(id), &mut pid) };
    (thread != 0 && pid != 0).then_some(pid)
}

/// Executable file name (e.g. `Code.exe`) of a process, if we may query it.
pub fn process_exe_name(pid: u32) -> Option<String> {
    let mut buf = [0u16; 1024];
    let mut len = buf.len() as u32;
    // SAFETY: the handle is closed before returning; `buf`/`len` describe a
    // live buffer and QueryFullProcessImageNameW writes at most `len` units.
    let ok = unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return None;
        }
        let ok =
            QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len);
        CloseHandle(process);
        ok
    };
    if ok == 0 {
        return None;
    }
    let path = String::from_utf16_lossy(&buf[..len as usize]);
    Some(exe_file_name(&path).to_owned())
}

/// Window title (truncated to 512 UTF-16 units). Reading another process's
/// title this way uses the cached caption and does not block on hung apps.
pub fn window_title(id: WindowId) -> String {
    let mut buf = [0u16; 513];
    // SAFETY: `buf` is live and its length (in u16 units) is passed.
    let n = unsafe {
        if GetWindowTextLengthW(hwnd(id)) == 0 {
            return String::new();
        }
        GetWindowTextW(hwnd(id), buf.as_mut_ptr(), buf.len() as i32)
    };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn window_rect(id: WindowId) -> Option<ScreenRect> {
    let mut r = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    // SAFETY: `r` is a live local RECT.
    (unsafe { GetWindowRect(hwnd(id), &mut r) } != 0).then(|| r.into())
}

struct MonitorInfo {
    monitor: ScreenRect,
    work: ScreenRect,
}

fn monitor_of(id: WindowId) -> Option<MonitorInfo> {
    // SAFETY: MONITORINFO is plain data and is zero-valid; cbSize is set as
    // required; dpi out-params are live locals.
    unsafe {
        let mon = MonitorFromWindow(hwnd(id), MONITOR_DEFAULTTONEAREST);
        if mon.is_null() {
            return None;
        }
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(mon, &mut info) == 0 {
            return None;
        }
        Some(MonitorInfo {
            monitor: info.rcMonitor.into(),
            work: info.rcWork.into(),
        })
    }
}

/// Decide whether a window of a supported app is a main window ready to be
/// placed. Returns a short reason for logs.
pub fn readiness(id: WindowId, rule: &Rule) -> (Readiness, &'static str) {
    let h = hwnd(id);
    // SAFETY: all calls take only the HWND by value; see module invariants.
    let (visible, style, ex_style, owned, iconic, zoomed) = unsafe {
        (
            IsWindowVisible(h) != 0,
            GetWindowLongPtrW(h, GWL_STYLE) as u32,
            GetWindowLongPtrW(h, GWL_EXSTYLE) as u32,
            !GetWindow(h, GW_OWNER).is_null(),
            IsIconic(h) != 0,
            IsZoomed(h) != 0,
        )
    };
    if !visible {
        return (Readiness::NotYet, "hidden");
    }
    // Visible in user32 can still mean hidden on a different virtual desktop.
    let mut cloaked: u32 = 0;
    // SAFETY: cloaked is a live u32 and its exact size is passed to DWM.
    unsafe {
        DwmGetWindowAttribute(
            h,
            DWMWA_CLOAKED as u32,
            &mut cloaked as *mut u32 as *mut _,
            size_of::<u32>() as u32,
        );
    }
    if cloaked != 0 {
        return (Readiness::Never, "cloaked window");
    }
    if style & WS_CHILD != 0 || owned {
        return (Readiness::Never, "child or owned window");
    }
    if ex_style & WS_EX_TOOLWINDOW != 0 {
        return (Readiness::Never, "tool window");
    }
    if ex_style & WS_EX_TOPMOST != 0 {
        return (Readiness::Never, "topmost window");
    }
    let title = window_title(id);
    if title.is_empty() {
        return (Readiness::NotYet, "untitled");
    }
    if !title_matches(rule, &title) {
        return (Readiness::NotYet, "title filter not matched");
    }
    if iconic {
        return (Readiness::Never, "minimized");
    }
    if zoomed {
        return (Readiness::Never, "maximized");
    }
    let (Some(rect), Some(mon)) = (window_rect(id), monitor_of(id)) else {
        return (Readiness::NotYet, "window or monitor lookup failed");
    };
    if rect.covers(&mon.monitor) {
        return (Readiness::Never, "fullscreen");
    }
    (Readiness::Ready, "ready")
}

/// Where a window is now and where its rule wants it on its current monitor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plan {
    pub current: ScreenRect,
    pub target: ScreenRect,
}

pub fn plan(id: WindowId, rect: PlacementRect) -> Option<Plan> {
    let current = window_rect(id)?;
    let mon = monitor_of(id)?;
    Some(Plan {
        current,
        target: target_rect(rect, mon.monitor, mon.work),
    })
}

/// Move/resize without activating or changing Z-order. Asynchronous so a hung
/// target app cannot block our thread.
pub fn move_window(id: WindowId, to: ScreenRect) -> PlaceOutcome {
    let flags = SWP_NOZORDER | SWP_NOOWNERZORDER | SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS;
    // SAFETY: HWND by value; null insert-after is ignored with SWP_NOZORDER.
    let ok = unsafe {
        SetWindowPos(
            hwnd(id),
            null_mut(),
            to.left,
            to.top,
            to.width(),
            to.height(),
            flags,
        )
    };
    if ok != 0 {
        return PlaceOutcome::Placed;
    }
    // SAFETY: reads this thread's last-error value.
    match unsafe { GetLastError() } {
        ERROR_ACCESS_DENIED | ERROR_INVALID_WINDOW_HANDLE => PlaceOutcome::Failed,
        _ => PlaceOutcome::Transient,
    }
}

/// All current top-level windows (unfiltered).
pub fn top_level_windows() -> Vec<WindowId> {
    unsafe extern "system" fn collect(h: HWND, lparam: LPARAM) -> windows_sys::core::BOOL {
        // SAFETY: `lparam` is the `&mut Vec` passed below, alive for the
        // whole synchronous EnumWindows call.
        let list = unsafe { &mut *(lparam as *mut Vec<WindowId>) };
        list.push(h as WindowId);
        1
    }
    let mut list: Vec<WindowId> = Vec::new();
    // SAFETY: see `collect`.
    unsafe {
        EnumWindows(Some(collect), &mut list as *mut Vec<WindowId> as LPARAM);
    }
    list
}

// ---------------------------------------------------------------------------
// WinEvent hooks and the message loop

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WinEvent {
    /// Top-level window shown or renamed: it may have become ready.
    Changed(WindowId),
    Destroyed(WindowId),
    /// A bounded queue overflowed: reconcile once instead of losing windows.
    Rescan,
}

/// Program messages posted to our own thread to wake `GetMessageW`.
const WM_EVENTS_QUEUED: u32 = WM_APP + 1;
const MAX_QUEUED_EVENTS: usize = 4096;

thread_local! {
    static QUEUE: RefCell<Vec<WinEvent>> = const { RefCell::new(Vec::new()) };
    static OVERFLOW: Cell<bool> = const { Cell::new(false) };
}
static MAIN_THREAD: AtomicU32 = AtomicU32::new(0);

unsafe extern "system" fn on_win_event(
    _hook: HWINEVENTHOOK,
    event: u32,
    h: HWND,
    id_object: i32,
    id_child: i32,
    _thread: u32,
    _time: u32,
) {
    // Reject everything that is not a window object itself (carets, menus,
    // controls' accessible children, ...) before doing any work.
    if h.is_null() || id_object != OBJID_WINDOW || id_child != CHILDID_SELF as i32 {
        return;
    }
    let ev = match event {
        EVENT_OBJECT_DESTROY => WinEvent::Destroyed(h as WindowId),
        EVENT_OBJECT_SHOW | EVENT_OBJECT_NAMECHANGE => {
            // SAFETY: HWND by value; reject child controls cheaply.
            if unsafe { GetWindowLongPtrW(h, GWL_STYLE) } as u32 & WS_CHILD != 0 {
                return;
            }
            WinEvent::Changed(h as WindowId)
        }
        _ => return,
    };
    let wake = QUEUE.with_borrow_mut(|q| {
        if q.len() >= MAX_QUEUED_EVENTS {
            OVERFLOW.set(true);
            return false;
        }
        q.push(ev);
        q.len() == 1
    });
    if wake {
        // SAFETY: no pointers; posting to our own thread's queue.
        unsafe {
            PostThreadMessageW(GetCurrentThreadId(), WM_EVENTS_QUEUED, 0, 0);
        }
    }
}

/// Installed WinEvent hooks; unhooked on drop.
pub struct Hooks(Vec<HWINEVENTHOOK>);

impl Drop for Hooks {
    fn drop(&mut self) {
        for &h in &self.0 {
            // SAFETY: each handle came from a successful SetWinEventHook.
            unsafe {
                UnhookWinEvent(h);
            }
        }
    }
}

/// Install out-of-context hooks for DESTROY+SHOW and NAMECHANGE only.
/// Must be called on the thread that runs [`next_message`].
pub fn install_hooks() -> Result<Hooks, u32> {
    // Create the thread queue before a hook or console handler can post to it.
    // SAFETY: MSG is zero-valid and remains live throughout PeekMessageW.
    unsafe {
        let mut msg: MSG = std::mem::zeroed();
        PeekMessageW(&mut msg, null_mut(), 0, 0, PM_NOREMOVE);
    }
    // SAFETY: no arguments.
    MAIN_THREAD.store(unsafe { GetCurrentThreadId() }, Ordering::Relaxed);
    let mut hooks = Hooks(Vec::new());
    let flags = WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS;
    for (min, max) in [
        (EVENT_OBJECT_DESTROY, EVENT_OBJECT_SHOW),
        (EVENT_OBJECT_NAMECHANGE, EVENT_OBJECT_NAMECHANGE),
    ] {
        // SAFETY: out-of-context hook with a 'static callback; no module.
        let h = unsafe { SetWinEventHook(min, max, null_mut(), Some(on_win_event), 0, 0, flags) };
        if h.is_null() {
            // SAFETY: reads last-error; `hooks` drop unhooks any installed.
            return Err(unsafe { GetLastError() });
        }
        hooks.0.push(h);
    }
    Ok(hooks)
}

/// Drain WinEvents queued by the hook callback.
pub fn take_events() -> Vec<WinEvent> {
    let mut events = QUEUE.with_borrow_mut(std::mem::take);
    if OVERFLOW.replace(false) {
        events.push(WinEvent::Rescan);
    }
    events
}

pub enum Message {
    EventsQueued,
    Timer(usize),
    Quit,
    Error(u32),
}

/// Block until something relevant happens. Hook callbacks run inside here.
pub fn next_message() -> Message {
    loop {
        // SAFETY: MSG is plain data and zero-valid; `msg` is a live local.
        let mut msg: MSG = unsafe { std::mem::zeroed() };
        let r = unsafe { GetMessageW(&mut msg, null_mut(), 0, 0) };
        if r == -1 {
            // SAFETY: GetMessageW just failed on this thread.
            return Message::Error(unsafe { GetLastError() });
        }
        if r == 0 {
            return Message::Quit;
        }
        match msg.message {
            WM_EVENTS_QUEUED => return Message::EventsQueued,
            WM_TIMER if msg.hwnd.is_null() => return Message::Timer(msg.wParam),
            // SAFETY: `msg` was filled by GetMessageW.
            _ => unsafe {
                DispatchMessageW(&msg);
            },
        }
    }
}

/// Arm a one-shot thread timer (caller kills it when it fires).
pub fn set_timer(delay_ms: u32) -> Option<usize> {
    // SAFETY: thread timer (null HWND, no callback) -> WM_TIMER in our queue.
    let id = unsafe { SetTimer(null_mut(), 0, delay_ms, None) };
    (id != 0).then_some(id)
}

pub fn kill_timer(id: usize) {
    // SAFETY: id came from SetTimer on this thread; double-kill is harmless.
    unsafe {
        KillTimer(null_mut(), id);
    }
}

/// Ctrl+C / Ctrl+Break / console close: ask the message loop to exit so
/// hooks are unhooked cleanly.
pub fn install_ctrl_handler() {
    unsafe extern "system" fn handler(_ctrl: u32) -> windows_sys::core::BOOL {
        let tid = MAIN_THREAD.load(Ordering::Relaxed);
        if tid == 0 {
            return 0;
        }
        // SAFETY: no pointers; runs on a system-created thread.
        unsafe { PostThreadMessageW(tid, WM_QUIT, 0, 0) }
    }
    // SAFETY: registers a 'static function.
    unsafe {
        SetConsoleCtrlHandler(Some(handler), 1);
    }
}

// ---------------------------------------------------------------------------
// Single-instance lock for apply mode

/// Held named mutex; released on drop (or by the OS on exit).
pub struct InstanceLock(HANDLE);

impl Drop for InstanceLock {
    fn drop(&mut self) {
        // SAFETY: handle came from CreateMutexW and is closed only here.
        unsafe {
            CloseHandle(self.0);
        }
    }
}

/// `Ok(None)` if another apply-mode instance already runs in this session.
pub fn acquire_instance_lock() -> Result<Option<InstanceLock>, u32> {
    acquire_named_lock("Local\\window-placer.apply.v1")
}

fn acquire_named_lock(name: &str) -> Result<Option<InstanceLock>, u32> {
    let name: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
    // SAFETY: `name` is NUL-terminated and outlives the call; the returned
    // handle is owned by InstanceLock or closed immediately.
    unsafe {
        let h = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
        if h.is_null() {
            return Err(GetLastError());
        }
        if GetLastError() == ERROR_ALREADY_EXISTS {
            CloseHandle(h);
            return Ok(None);
        }
        Ok(Some(InstanceLock(h)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instance_lock_excludes_duplicates_and_releases() {
        let name = format!("Local\\window-placer.test.{}", std::process::id());
        let first = acquire_named_lock(&name).unwrap().unwrap();
        assert!(acquire_named_lock(&name).unwrap().is_none());
        drop(first);
        assert!(acquire_named_lock(&name).unwrap().is_some());
    }

    #[test]
    fn queue_overflow_requests_one_reconciliation() {
        QUEUE.with_borrow_mut(|q| *q = vec![WinEvent::Destroyed(1); MAX_QUEUED_EVENTS]);
        // SAFETY: DESTROY processing treats HWND only as an opaque value and
        // does not inspect or manipulate this deliberately invalid test handle.
        unsafe {
            on_win_event(
                null_mut(),
                EVENT_OBJECT_DESTROY,
                1 as HWND,
                OBJID_WINDOW,
                CHILDID_SELF as i32,
                0,
                0,
            );
        }
        let events = take_events();
        assert_eq!(events.len(), MAX_QUEUED_EVENTS + 1);
        assert_eq!(events.last(), Some(&WinEvent::Rescan));
        assert!(take_events().is_empty());
    }
}
