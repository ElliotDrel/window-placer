//! Command line, logging and the single-threaded event loop.

use std::cell::Cell;
use std::collections::HashMap;
use std::fmt;
use std::io::Write;

use crate::rules::{self, Rule};
use crate::tracker::{PlaceOutcome, Readiness, Schedule, Tracker};
use crate::win::{self, Message, WinEvent, WindowId};
use std::path::PathBuf;

pub const USAGE: &str = "\
window-placer: place supported app windows once, when they first become ready.

USAGE: window-placer [--apply] [--config PATH] [--duration SECONDS]

  (default)            dry run: log what would be moved, move nothing
  --config PATH        rules file (default: windows.conf beside the executable)
  --apply              really move windows (quiet; one apply instance per session)
  --duration SECONDS   exit after SECONDS (default: run until stopped or logoff)
  -h, --help           show this help
";

const MAX_DURATION_SECS: u32 = 2_000_000; // SetTimer limit is ~24.8 days
const MAX_LOG_LINES: u32 = 300;
const MIN_PRUNE_AT: usize = 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    pub apply: bool,
    pub duration_secs: Option<u32>,
    pub config: Option<PathBuf>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Run(Options),
    Help,
}

pub fn parse_args<I: IntoIterator<Item = String>>(args: I) -> Result<Command, String> {
    let mut opts = Options {
        apply: false,
        duration_secs: None,
        config: None,
    };
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" | "/?" => return Ok(Command::Help),
            "--apply" => opts.apply = true,
            "--config" => {
                opts.config = Some(PathBuf::from(args.next().ok_or("--config needs a path")?))
            }
            "--duration" => {
                let value = args.next().ok_or("--duration needs a value in seconds")?;
                let secs: u32 = value
                    .parse()
                    .map_err(|_| format!("invalid --duration value: {value}"))?;
                if secs == 0 || secs > MAX_DURATION_SECS {
                    return Err(format!(
                        "--duration must be between 1 and {MAX_DURATION_SECS} seconds"
                    ));
                }
                opts.duration_secs = Some(secs);
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(Command::Run(opts))
}

/// Bounded stdout logging; disabled entirely in apply mode.
struct Log {
    enabled: bool,
    remaining: Cell<u32>,
}

impl Log {
    fn line(&self, args: fmt::Arguments) {
        if !self.enabled {
            return;
        }
        let left = self.remaining.get();
        if left == 0 {
            return;
        }
        self.remaining.set(left - 1);
        let mut out = std::io::stdout().lock();
        // Write errors (no console, closed pipe) are deliberately ignored.
        let _ = if left == 1 {
            writeln!(out, "log limit reached; further messages suppressed")
        } else {
            writeln!(out, "{args}")
        };
    }
}

fn short(title: &str) -> String {
    const MAX: usize = 60;
    if title.chars().count() <= MAX {
        title.to_owned()
    } else {
        title.chars().take(MAX).chain("...".chars()).collect()
    }
}

struct App {
    apply: bool,
    rules: Vec<Rule>,
    log: Log,
    tracker: Tracker,
    /// Armed one-shot timers: timer id -> (window, tracker token).
    timers: HashMap<usize, (WindowId, u64)>,
    prune_at: usize,
    timer_failed: bool,
}

impl App {
    fn on_changed(&mut self, hwnd: WindowId) {
        let Some(pid) = win::window_pid(hwnd) else {
            return;
        };
        let log = &self.log;
        let rules = &self.rules;
        let schedule = self.tracker.on_window_event(
            hwnd,
            pid,
            || {
                let exe = win::process_exe_name(pid)?;
                let rule = crate::rules::rule_for_exe(rules, &exe)?;
                log.line(format_args!("{exe} pid {pid} hwnd {hwnd:#x}: tracking"));
                Some(rule)
            },
            |rule| {
                let exe = &rules[rule].exe;
                let (r, why) = win::readiness(hwnd, &rules[rule]);
                match r {
                    Readiness::Ready => log.line(format_args!(
                        "{exe} hwnd {hwnd:#x}: ready \"{}\"",
                        short(&win::window_title(hwnd))
                    )),
                    Readiness::Never => {
                        log.line(format_args!("{exe} hwnd {hwnd:#x}: left alone ({why})"))
                    }
                    Readiness::NotYet => {}
                }
                r
            },
        );
        self.arm(hwnd, schedule);
        if self.tracker.len() > self.prune_at {
            self.tracker.prune(|h, pid| win::window_pid(h) == Some(pid));
            self.prune_at = (self.tracker.len() * 2).max(MIN_PRUNE_AT);
        }
    }

    fn on_timer(&mut self, hwnd: WindowId, token: u64) {
        let (log, apply) = (&self.log, self.apply);
        let rules = &self.rules;
        let schedule = self.tracker.on_timer(
            hwnd,
            token,
            win::window_pid(hwnd),
            |rule| {
                let (r, why) = win::readiness(hwnd, &rules[rule]);
                if r != Readiness::Ready {
                    log.line(format_args!(
                        "{} hwnd {hwnd:#x}: not placed ({why})",
                        rules[rule].exe
                    ));
                }
                r
            },
            |rule| place(hwnd, &rules[rule], apply, log),
        );
        self.arm(hwnd, schedule);
    }

    fn arm(&mut self, hwnd: WindowId, schedule: Option<Schedule>) {
        let Some(s) = schedule else { return };
        match win::set_timer(s.delay_ms) {
            Some(id) => {
                self.timers.insert(id, (hwnd, s.token));
            }
            None => {
                eprintln!("window-placer: could not arm placement timer");
                self.timer_failed = true;
            }
        }
    }
}

fn place(hwnd: WindowId, rule: &Rule, apply: bool, log: &Log) -> PlaceOutcome {
    let exe = &rule.exe;
    let Some(plan) = win::plan(hwnd, rule.rect) else {
        log.line(format_args!("{exe} hwnd {hwnd:#x}: monitor query failed"));
        return PlaceOutcome::Transient;
    };
    let (c, t) = (plan.current, plan.target);
    if c == t {
        log.line(format_args!("{exe} hwnd {hwnd:#x}: already in place"));
        return PlaceOutcome::Placed;
    }
    let verb = if apply { "moving" } else { "would move" };
    log.line(format_args!(
        "{exe} hwnd {hwnd:#x}: {verb} {},{} {}x{} -> {},{} {}x{}",
        c.left,
        c.top,
        c.width(),
        c.height(),
        t.left,
        t.top,
        t.width(),
        t.height()
    ));
    if !apply {
        return PlaceOutcome::Placed;
    }
    win::move_window(hwnd, t)
}

/// Run until WM_QUIT, Ctrl+C or `--duration` elapses. Returns an exit code.
pub fn run(opts: Options) -> i32 {
    let config = opts.config.clone().unwrap_or_else(|| {
        std::env::current_exe()
            .unwrap_or_default()
            .with_file_name("windows.conf")
    });
    let rules = match rules::load(&config) {
        Ok(rules) => rules,
        Err(error) => {
            eprintln!("window-placer: {error}");
            return 1;
        }
    };
    win::enable_per_monitor_dpi();

    // Only apply mode is exclusive, so dry runs never conflict with it.
    let _lock = if opts.apply {
        match win::acquire_instance_lock() {
            Ok(Some(lock)) => Some(lock),
            Ok(None) => {
                eprintln!("window-placer: another --apply instance is already running");
                return 2;
            }
            Err(code) => {
                eprintln!("window-placer: could not create instance mutex (error {code})");
                return 1;
            }
        }
    } else {
        None
    };

    let hooks = match win::install_hooks() {
        Ok(h) => h,
        Err(code) => {
            eprintln!("window-placer: SetWinEventHook failed (error {code})");
            return 1;
        }
    };
    win::install_ctrl_handler();

    let mut app = App {
        apply: opts.apply,
        rules,
        log: Log {
            enabled: !opts.apply,
            remaining: Cell::new(MAX_LOG_LINES),
        },
        tracker: Tracker::new(),
        timers: HashMap::new(),
        prune_at: MIN_PRUNE_AT,
        timer_failed: false,
    };
    app.log.line(format_args!(
        "dry run: nothing will be moved (pass --apply to move windows){}",
        match opts.duration_secs {
            Some(s) => format!("; exiting after {s}s"),
            None => "; Ctrl+C to stop".to_owned(),
        }
    ));

    // Hooks are installed first so no window can slip between the two.
    for hwnd in win::top_level_windows() {
        app.on_changed(hwnd);
    }

    let quit_timer = opts
        .duration_secs
        .and_then(|s| win::set_timer(s.saturating_mul(1000)));
    if opts.duration_secs.is_some() && quit_timer.is_none() {
        eprintln!("window-placer: could not arm --duration timer");
        return 1;
    }

    let exit_code = loop {
        if app.timer_failed {
            break 1;
        }
        match win::next_message() {
            Message::Quit => break 0,
            Message::Error(code) => {
                eprintln!("window-placer: GetMessageW failed (error {code})");
                break 1;
            }
            Message::EventsQueued => {
                for ev in win::take_events() {
                    match ev {
                        WinEvent::Changed(h) => app.on_changed(h),
                        WinEvent::Destroyed(h) => app.tracker.on_destroy(h),
                        WinEvent::Rescan => {
                            app.tracker.prune(|h, pid| win::window_pid(h) == Some(pid));
                            for h in win::top_level_windows() {
                                app.on_changed(h);
                            }
                        }
                    }
                }
            }
            Message::Timer(id) => {
                win::kill_timer(id);
                if Some(id) == quit_timer {
                    break 0;
                }
                if let Some((hwnd, token)) = app.timers.remove(&id) {
                    app.on_timer(hwnd, token);
                }
            }
        }
    };

    for &id in app.timers.keys() {
        win::kill_timer(id);
    }
    if let Some(id) = quit_timer {
        win::kill_timer(id);
    }
    drop(hooks);
    app.log.line(format_args!("stopped"));
    exit_code
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Command, String> {
        parse_args(args.iter().map(|s| s.to_string()))
    }

    #[test]
    fn default_is_dry_run_forever() {
        assert_eq!(
            parse(&[]),
            Ok(Command::Run(Options {
                apply: false,
                duration_secs: None,
                config: None
            }))
        );
    }

    #[test]
    fn apply_and_duration() {
        assert_eq!(
            parse(&["--duration", "5", "--apply"]),
            Ok(Command::Run(Options {
                apply: true,
                duration_secs: Some(5),
                config: None
            }))
        );
    }

    #[test]
    fn explicit_config_path() {
        let Command::Run(opts) = parse(&["--config", "C:\\my rules\\windows.conf"]).unwrap() else {
            panic!()
        };
        assert_eq!(
            opts.config,
            Some(PathBuf::from("C:\\my rules\\windows.conf"))
        );
    }

    #[test]
    fn help_wins() {
        assert_eq!(parse(&["--apply", "--help"]), Ok(Command::Help));
        assert_eq!(parse(&["-h"]), Ok(Command::Help));
    }

    #[test]
    fn rejects_bad_input() {
        assert!(parse(&["--config"]).is_err());
        assert!(parse(&["--duration"]).is_err());
        assert!(parse(&["--duration", "abc"]).is_err());
        assert!(parse(&["--duration", "0"]).is_err());
        assert!(parse(&["--duration", "-1"]).is_err());
        assert!(parse(&["--duration", "999999999"]).is_err());
        assert!(parse(&["--aply"]).is_err());
    }

    #[test]
    fn titles_are_shortened() {
        assert_eq!(short("abc"), "abc");
        let long = "x".repeat(100);
        assert_eq!(short(&long).chars().count(), 63);
    }
}
