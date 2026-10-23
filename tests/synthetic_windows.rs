//! Win32 integration tests. They create, inspect and move ONLY windows owned
//! by this test process; no other window on the desktop is touched.

#![cfg(windows)]

use std::ptr::{null, null_mut};
use std::sync::Once;

use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::GetCurrentProcessId;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetForegroundWindow,
    GetWindowRect, MSG, PM_REMOVE, PeekMessageW, RegisterClassW, SW_SHOWMINNOACTIVE, SW_SHOWNA,
    ShowWindow, WNDCLASSW, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_MAXIMIZE, WS_OVERLAPPEDWINDOW,
};

use window_placer::rules::{PlacementRect, Rule};
use window_placer::tracker::{PlaceOutcome, Readiness};
use window_placer::win::{self, WindowId};

const CLASS: &str = "window-placer-test-window\0";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn register_class() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        // Same DPI mode as the real program, before any test window exists.
        win::enable_per_monitor_dpi();
        let class: Vec<u16> = CLASS.encode_utf16().collect();
        // SAFETY: WNDCLASSW is plain data; the class name is leaked so it
        // stays valid for the life of the process.
        unsafe {
            let mut wc: WNDCLASSW = std::mem::zeroed();
            wc.lpfnWndProc = Some(DefWindowProcW);
            wc.hInstance = GetModuleHandleW(null());
            wc.lpszClassName = Box::leak(class.into_boxed_slice()).as_ptr();
            assert_ne!(RegisterClassW(&wc), 0);
        }
    });
}

/// A test-owned top-level window, destroyed on drop.
struct TestWindow(HWND);

impl TestWindow {
    fn new(title: &str, ex_style: u32, owner: HWND, show: Option<i32>) -> Self {
        Self::with_style(title, ex_style, WS_OVERLAPPEDWINDOW, owner, show)
    }

    fn with_style(title: &str, ex_style: u32, style: u32, owner: HWND, show: Option<i32>) -> Self {
        register_class();
        let class = wide(CLASS.trim_end_matches('\0'));
        let title = wide(title);
        // SAFETY: strings are NUL-terminated and outlive the call.
        let h = unsafe {
            CreateWindowExW(
                ex_style,
                class.as_ptr(),
                title.as_ptr(),
                style,
                40,
                40,
                220,
                160,
                owner,
                null_mut(),
                GetModuleHandleW(null()),
                null(),
            )
        };
        assert!(!h.is_null());
        if let Some(cmd) = show {
            // SAFETY: our own window; SW_SHOWNA / SW_SHOWMINNOACTIVE do not activate.
            unsafe { ShowWindow(h, cmd) };
        }
        pump();
        TestWindow(h)
    }

    fn visible(title: &str) -> Self {
        Self::new(title, 0, null_mut(), Some(SW_SHOWNA))
    }

    fn id(&self) -> WindowId {
        self.0 as WindowId
    }

    fn rect(&self) -> RECT {
        let mut r = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        // SAFETY: our own window and a live local.
        unsafe { GetWindowRect(self.0, &mut r) };
        r
    }
}

impl Drop for TestWindow {
    fn drop(&mut self) {
        // SAFETY: created on this thread; destroyed once.
        unsafe { DestroyWindow(self.0) };
    }
}

fn pump() {
    // SAFETY: standard non-blocking message pump for this thread's windows.
    unsafe {
        let mut msg: MSG = std::mem::zeroed();
        for _ in 0..200 {
            if PeekMessageW(&mut msg, null_mut(), 0, 0, PM_REMOVE) == 0 {
                break;
            }
            DispatchMessageW(&msg);
        }
    }
}

fn rule(title_contains: Option<&'static str>) -> Rule {
    Rule {
        exe: "test.exe".into(),
        title_contains: title_contains.map(str::to_owned),
        rect: PlacementRect {
            x: 10,
            y: 20,
            width: 300,
            height: 200,
        },
    }
}

#[test]
fn identity_and_enumeration() {
    let w = TestWindow::visible("wp identity");
    // SAFETY: no arguments.
    let me = unsafe { GetCurrentProcessId() };
    assert_eq!(win::window_pid(w.id()), Some(me));
    let exe = win::process_exe_name(me).unwrap();
    assert!(exe.to_ascii_lowercase().ends_with(".exe"), "{exe}");
    assert!(win::top_level_windows().contains(&w.id()));
    assert_eq!(win::window_title(w.id()), "wp identity");

    let id = w.id();
    drop(w);
    assert_eq!(win::window_pid(id), None);
}

#[test]
fn readiness_filters_main_windows() {
    let r = rule(None);
    let normal = TestWindow::visible("wp normal");
    assert_eq!(win::readiness(normal.id(), &r).0, Readiness::Ready);

    let hidden = TestWindow::new("wp hidden", 0, null_mut(), None);
    assert_eq!(win::readiness(hidden.id(), &r).0, Readiness::NotYet);

    let untitled = TestWindow::visible("");
    assert_eq!(win::readiness(untitled.id(), &r).0, Readiness::NotYet);

    let tool = TestWindow::new("wp tool", WS_EX_TOOLWINDOW, null_mut(), Some(SW_SHOWNA));
    assert_eq!(win::readiness(tool.id(), &r).0, Readiness::NotYet);

    let topmost = TestWindow::new("wp topmost", WS_EX_TOPMOST, null_mut(), Some(SW_SHOWNA));
    assert_eq!(win::readiness(topmost.id(), &r).0, Readiness::NotYet);

    let owned = TestWindow::new("wp owned", 0, normal.0, Some(SW_SHOWNA));
    assert_eq!(win::readiness(owned.id(), &r).0, Readiness::NotYet);

    let minimized = TestWindow::new("wp min", 0, null_mut(), Some(SW_SHOWMINNOACTIVE));
    assert_eq!(win::readiness(minimized.id(), &r).0, Readiness::NotYet);
}

#[test]
fn title_filter_waits_for_matching_title() {
    let w = TestWindow::visible("Status");
    assert_eq!(
        win::readiness(w.id(), &rule(Some("Wispr Flow"))).0,
        Readiness::NotYet
    );
    let w = TestWindow::visible("Hub - Wispr Flow");
    assert_eq!(
        win::readiness(w.id(), &rule(Some("wispr flow"))).0,
        Readiness::Ready
    );
}

#[test]
fn moves_own_window_into_work_area_without_activating() {
    let w = TestWindow::visible("wp move");
    let plan = win::plan(w.id(), rule(None).rect).unwrap();
    assert_eq!(win::move_window(w.id(), plan.target), PlaceOutcome::Placed);
    // The move is asynchronous (posted to the owning thread, which is us).
    pump();
    let r = w.rect();
    assert_eq!(
        (r.left, r.top, r.right, r.bottom),
        (
            plan.target.left,
            plan.target.top,
            plan.target.right,
            plan.target.bottom
        )
    );
    // SAFETY: no arguments.
    assert_ne!(unsafe { GetForegroundWindow() }, w.0);
}

#[test]
fn moving_a_destroyed_window_fails_without_retry() {
    let w = TestWindow::visible("wp gone");
    let id = w.id();
    let target = win::plan(id, rule(None).rect).unwrap().target;
    drop(w);
    assert_eq!(win::move_window(id, target), PlaceOutcome::Failed);
}

#[test]
fn fullscreen_and_maximized_windows_are_protected_without_focus_change() {
    let r = rule(None);
    let maximized = TestWindow::with_style(
        "wp maximized",
        0,
        WS_OVERLAPPEDWINDOW | WS_MAXIMIZE,
        null_mut(),
        Some(SW_SHOWNA),
    );
    assert_eq!(win::readiness(maximized.id(), &r).0, Readiness::Never);
    assert_eq!(win::readiness(maximized.id(), &r).1, "maximized");

    let full = TestWindow::visible("wp fullscreen");
    // SAFETY: valid test-owned HWND and correctly sized MONITORINFO.
    let bounds = unsafe {
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = size_of::<MONITORINFO>() as u32;
        let mon = MonitorFromWindow(full.0, MONITOR_DEFAULTTONEAREST);
        assert_ne!(GetMonitorInfoW(mon, &mut info), 0);
        info.rcMonitor.into()
    };
    assert_eq!(win::move_window(full.id(), bounds), PlaceOutcome::Placed);
    pump();
    assert_eq!(
        win::readiness(full.id(), &r),
        (Readiness::Never, "fullscreen")
    );
    // SAFETY: reading foreground identity only; neither test window was activated.
    assert_ne!(unsafe { GetForegroundWindow() }, full.0);
    assert_ne!(unsafe { GetForegroundWindow() }, maximized.0);
}

#[test]
fn fullscreen_with_a_different_title_cannot_bypass_final_safety_gate() {
    let w = TestWindow::visible("wp protected");
    let original = win::plan(w.id(), rule(None).rect).unwrap();
    let work = unsafe {
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = size_of::<MONITORINFO>() as u32;
        assert_ne!(
            GetMonitorInfoW(MonitorFromWindow(w.0, MONITOR_DEFAULTTONEAREST), &mut info),
            0
        );
        info.rcMonitor.into()
    };
    win::move_window(w.id(), work);
    pump();
    let before = w.rect();
    let filtered = rule(Some("different startup title"));
    assert_eq!(win::readiness(w.id(), &filtered).0, Readiness::Never);
    assert_eq!(
        win::move_if_ready(w.id(), &filtered, original.target),
        PlaceOutcome::Failed
    );
    pump();
    let after = w.rect();
    assert_eq!(
        (before.left, before.top, before.right, before.bottom),
        (after.left, after.top, after.right, after.bottom)
    );
}

#[test]
fn temporary_tool_window_can_become_an_eligible_main_window() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GWL_EXSTYLE, SetWindowLongPtrW};
    let w = TestWindow::new("Wispr Flow", WS_EX_TOOLWINDOW, null_mut(), Some(SW_SHOWNA));
    let r = rule(Some("Wispr Flow"));
    let mut tracker = window_placer::tracker::Tracker::new();
    let pid = win::window_pid(w.id()).unwrap();
    assert_eq!(
        tracker.on_window_event(w.id(), pid, || Some(0), |_| win::readiness(w.id(), &r).0),
        None
    );
    unsafe {
        SetWindowLongPtrW(w.0, GWL_EXSTYLE, 0);
    }
    assert!(
        tracker
            .on_window_event(
                w.id(),
                pid,
                || unreachable!(),
                |_| win::readiness(w.id(), &r).0
            )
            .is_some()
    );
}

#[test]
fn restored_work_area_sized_window_is_eligible() {
    let w = TestWindow::visible("wp restored");
    let area: window_placer::geometry::ScreenRect = unsafe {
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = size_of::<MONITORINFO>() as u32;
        assert_ne!(
            GetMonitorInfoW(MonitorFromWindow(w.0, MONITOR_DEFAULTTONEAREST), &mut info),
            0
        );
        let mut area: window_placer::geometry::ScreenRect = info.rcWork.into();
        let monitor = info.rcMonitor.into();
        // Some CI desktops have no taskbar. Keep this restored window smaller
        // than the full monitor so it unambiguously is not fullscreen.
        if area == monitor {
            area.bottom -= 1;
        }
        area
    };
    assert_eq!(win::move_window(w.id(), area), PlaceOutcome::Placed);
    pump();
    assert_eq!(win::readiness(w.id(), &rule(None)).0, Readiness::Ready);
    let target = win::plan(w.id(), rule(None).rect).unwrap().target;
    assert_eq!(
        win::move_if_ready(w.id(), &rule(None), target),
        PlaceOutcome::Placed
    );
}
