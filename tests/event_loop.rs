//! Exercise real cross-process WinEvents, thread timers and once-only placement.
//! The child fixture owns the ONLY window that this test is allowed to move.
#![cfg(windows)]

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::ptr::{null, null_mut};

use window_placer::geometry::ScreenRect;
use window_placer::rules::{PlacementRect, Rule};
use window_placer::tracker::{PlaceOutcome, Tracker};
use window_placer::win::{self, Message, WinEvent, WindowId};
use windows_sys::Win32::Foundation::RECT;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

struct Fixture {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    window: WindowId,
}

impl Fixture {
    fn start() -> Self {
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "fixture_process", "--ignored", "--nocapture"])
            .env("WINDOW_PLACER_FIXTURE", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let mut output = BufReader::new(child.stdout.take().unwrap());
        let window = loop {
            let mut line = String::new();
            assert_ne!(
                output.read_line(&mut line).unwrap(),
                0,
                "fixture exited early"
            );
            if let Some((_, id)) = line.trim().split_once("HWND=") {
                break id.parse().unwrap();
            }
        };
        Self {
            child,
            input,
            output,
            window,
        }
    }

    fn send(&mut self, command: &str) {
        writeln!(self.input, "{command}").unwrap();
        self.input.flush().unwrap();
    }

    fn rect(&mut self) -> ScreenRect {
        self.send("rect");
        let mut line = String::new();
        self.output.read_line(&mut line).unwrap();
        let values: Vec<i32> = line
            .trim()
            .strip_prefix("RECT=")
            .unwrap()
            .split(',')
            .map(|v| v.parse().unwrap())
            .collect();
        ScreenRect {
            left: values[0],
            top: values[1],
            right: values[2],
            bottom: values[3],
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // Only this test's child process; never an existing app.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn wait_for_event(window: WindowId, destroyed: bool) {
    let timeout = win::set_timer(5000).unwrap();
    loop {
        match win::next_message() {
            Message::EventsQueued => {
                let found = win::take_events().iter().any(|e| match e {
                    WinEvent::Changed(h) => !destroyed && *h == window,
                    WinEvent::Destroyed(h) => destroyed && *h == window,
                    WinEvent::Rescan => false,
                });
                if found {
                    win::kill_timer(timeout);
                    return;
                }
            }
            Message::Timer(id) if id == timeout => {
                win::kill_timer(timeout);
                panic!("expected WinEvent not delivered");
            }
            Message::Quit | Message::Error(_) => panic!("message loop failed"),
            _ => {}
        }
    }
}

#[test]
fn cross_process_show_title_timer_move_and_destroy() {
    win::enable_per_monitor_dpi();
    let _hooks = win::install_hooks().unwrap();
    let mut child = Fixture::start();
    let h = child.window;
    let pid = win::window_pid(h).unwrap();
    assert_eq!(pid, child.child.id());
    let rule = Rule {
        exe: "fixture only".into(),
        title_contains: None,
        rect: PlacementRect {
            x: 90,
            y: 100,
            width: 400,
            height: 280,
        },
    };
    let mut tracker = Tracker::new();

    child.send("show");
    wait_for_event(h, false);
    // The first SHOW is intentionally untitled, so no timer is armed yet.
    assert_eq!(
        tracker.on_window_event(h, pid, || Some(0), |_| win::readiness(h, &rule).0),
        None
    );
    child.send("title");
    wait_for_event(h, false);
    let schedule = tracker
        .on_window_event(h, pid, || Some(0), |_| win::readiness(h, &rule).0)
        .unwrap();
    let timer = win::set_timer(schedule.delay_ms).unwrap();
    loop {
        match win::next_message() {
            Message::Timer(id) if id == timer => {
                win::kill_timer(timer);
                break;
            }
            Message::EventsQueued => {
                win::take_events();
            }
            Message::Quit | Message::Error(_) => panic!("message loop failed"),
            _ => {}
        }
    }
    let plan = win::plan(h, rule.rect).unwrap();
    let mut moved = false;
    assert_eq!(
        tracker.on_timer(
            h,
            schedule.token,
            win::window_pid(h),
            |_| win::readiness(h, &rule).0,
            |_| {
                moved = true;
                assert_eq!(win::move_window(h, plan.target), PlaceOutcome::Placed);
                PlaceOutcome::Placed
            }
        ),
        None
    );
    assert!(moved);
    assert_eq!(child.rect(), plan.target);

    child.send("manual");
    wait_for_event(h, false);
    let manual = child.rect();
    assert_ne!(manual, plan.target);
    assert_eq!(
        tracker.on_window_event(
            h,
            pid,
            || panic!("already classified"),
            |_| panic!("already placed")
        ),
        None
    );
    assert_eq!(child.rect(), manual);
    child.send("destroy");
    wait_for_event(h, true);
    tracker.on_destroy(h);
    assert!(tracker.is_empty());
}

#[test]
#[ignore = "child fixture launched only by the parent integration test"]
fn fixture_process() {
    if std::env::var_os("WINDOW_PLACER_FIXTURE").is_none() {
        return;
    }
    win::enable_per_monitor_dpi();
    let class: Vec<u16> = "window-placer-event-fixture\0".encode_utf16().collect();
    // SAFETY: class strings and registered callback remain alive for this test;
    // every HWND created/moved/destroyed here belongs to this fixture process.
    unsafe {
        let instance = GetModuleHandleW(null());
        let mut wc: WNDCLASSW = std::mem::zeroed();
        wc.lpfnWndProc = Some(DefWindowProcW);
        wc.hInstance = instance;
        wc.lpszClassName = class.as_ptr();
        assert_ne!(RegisterClassW(&wc), 0);
        let empty = [0u16];
        let h = CreateWindowExW(
            WS_EX_NOACTIVATE,
            class.as_ptr(),
            empty.as_ptr(),
            WS_OVERLAPPEDWINDOW,
            40,
            40,
            220,
            160,
            null_mut(),
            null_mut(),
            instance,
            null(),
        );
        assert!(!h.is_null());
        println!("HWND={}", h as WindowId);
        std::io::stdout().flush().unwrap();
        for line in std::io::stdin().lock().lines() {
            let command = line.unwrap();
            match command.as_str() {
                "show" => {
                    ShowWindow(h, SW_SHOWNA);
                }
                "title" => {
                    let text: Vec<u16> = "Fixture ready\0".encode_utf16().collect();
                    SetWindowTextW(h, text.as_ptr());
                }
                "manual" => {
                    SetWindowPos(
                        h,
                        null_mut(),
                        150,
                        160,
                        500,
                        340,
                        SWP_NOACTIVATE | SWP_NOZORDER,
                    );
                    let text: Vec<u16> = "Manually adjusted\0".encode_utf16().collect();
                    SetWindowTextW(h, text.as_ptr());
                }
                "destroy" => {
                    DestroyWindow(h);
                    break;
                }
                "rect" => {
                    let mut msg: MSG = std::mem::zeroed();
                    while PeekMessageW(&mut msg, null_mut(), 0, 0, PM_REMOVE) != 0 {
                        DispatchMessageW(&msg);
                    }
                    let mut rect: RECT = std::mem::zeroed();
                    assert_ne!(GetWindowRect(h, &mut rect), 0);
                    println!(
                        "RECT={},{},{},{}",
                        rect.left, rect.top, rect.right, rect.bottom
                    );
                    std::io::stdout().flush().unwrap();
                }
                _ => panic!("unknown fixture command"),
            }
        }
    }
}
