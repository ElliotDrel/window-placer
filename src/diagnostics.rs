//! Low-frequency, bounded background diagnostics. No timer or monitoring thread.
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TRACE: AtomicBool = AtomicBool::new(false);

static ENABLED: AtomicBool = AtomicBool::new(false);
const LIMIT: u64 = 65536;

pub fn set_trace(enabled: bool) {
    TRACE.store(enabled, Ordering::Relaxed);
}
pub fn trace_enabled() -> bool {
    TRACE.load(Ordering::Relaxed)
}
pub fn start() {
    ENABLED.store(true, Ordering::Relaxed);
    std::panic::set_hook(Box::new(|info| record(&format!("panic: {info}"))));
    record(&format!("starting version={}", env!("CARGO_PKG_VERSION")));
}

pub fn record(message: &str) {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    if let Ok(exe) = std::env::current_exe() {
        let _ = write_event(&exe.with_file_name("window-placer.log"), message);
    }
}

fn write_event(path: &Path, message: &str) -> std::io::Result<()> {
    if std::fs::metadata(path).is_ok_and(|m| m.len() > LIMIT) {
        std::fs::write(path, [])?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let message: String = message.chars().take(2048).collect();
    writeln!(file, "unix={seconds} pid={} {message}", std::process::id())
}

pub fn error(message: std::fmt::Arguments<'_>) {
    record(&message.to_string());
    // A missing/closed console must never turn an error report into a panic.
    let _ = writeln!(std::io::stderr().lock(), "{message}");
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn diagnostics_remain_bounded_and_include_latest_event() {
        let path = std::env::temp_dir().join(format!("wp-log-test-{}.log", std::process::id()));
        std::fs::write(&path, vec![b'x'; LIMIT as usize + 1]).unwrap();
        write_event(&path, "fresh event").unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("fresh event"));
        assert!(text.len() < 4096);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn diagnostic_io_failure_is_fallible_without_panicking() {
        let path = std::env::temp_dir().join(format!(
            "wp-no-such-folder-{}/events.log",
            std::process::id()
        ));
        assert!(write_event(&path, "event").is_err());
    }
}
