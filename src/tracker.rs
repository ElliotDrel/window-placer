//! Per-window lifecycle, independent of Win32 so it can be unit tested.
//!
//! Each top-level window we hear about gets one entry keyed by HWND value and
//! tagged with its owning PID (so a recycled HWND is detected as a new window).
//!
//! ```text
//!   unsupported exe ------------------------------> Ignored
//!   supported exe --> Waiting --ready--> Pending --timer--> Done
//!                       ^  |                |  ^
//!                       |  +--never--> Done |  +-- transient failure (bounded)
//!                       +---- not ready ----+
//! ```
//!
//! `Done` is terminal for the window's lifetime: no event or timer can make
//! us touch it again, so later manual adjustments are preserved.

use std::collections::HashMap;

/// Delay between a window looking ready and placing it, letting the app
/// finish its own startup sizing first.
pub const SETTLE_MS: u32 = 3000;
/// Delay before retrying a transient placement failure.
pub const RETRY_MS: u32 = 750;
/// Total placement attempts per window (first try + retries).
pub const MAX_ATTEMPTS: u32 = 3;

/// Result of inspecting a supported window's current state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Readiness {
    /// Eligible main window in a normal (restored) state.
    Ready,
    /// Temporarily ineligible (hidden, minimized, tool/owned, title); may change later.
    NotYet,
    /// Maximized/fullscreen/work-area filling: protect for its lifetime.
    Never,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaceOutcome {
    /// Moved, already in place, or (dry run) would have moved.
    Placed,
    /// Window vanished or access denied: give up without retrying.
    Failed,
    /// Possibly temporary failure: retry within the attempt budget.
    Transient,
}

/// Request to arm a one-shot timer that must call [`Tracker::on_timer`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Schedule {
    pub token: u64,
    pub delay_ms: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Ignored,
    Waiting {
        rule: usize,
    },
    Pending {
        rule: usize,
        token: u64,
        attempt: u32,
    },
    Done,
}

#[derive(Debug)]
struct Entry {
    pid: u32,
    state: State,
}

#[derive(Debug, Default)]
pub struct Tracker {
    entries: HashMap<isize, Entry>,
    next_token: u64,
}

impl Tracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn token(&mut self) -> u64 {
        self.next_token += 1;
        self.next_token
    }

    /// A window was found by enumeration, shown, or renamed.
    ///
    /// `classify` maps the window's process to a rule index (called once per
    /// window lifetime); `readiness` inspects the window for a rule.
    pub fn on_window_event(
        &mut self,
        hwnd: isize,
        pid: u32,
        classify: impl FnOnce() -> Option<usize>,
        readiness: impl FnOnce(usize) -> Readiness,
    ) -> Option<Schedule> {
        let known = matches!(self.entries.get(&hwnd), Some(e) if e.pid == pid);
        if !known {
            // New window, or a recycled HWND now owned by another process.
            let state = match classify() {
                Some(rule) => State::Waiting { rule },
                None => State::Ignored,
            };
            self.entries.insert(hwnd, Entry { pid, state });
        }
        let rule = match self.entries[&hwnd].state {
            State::Waiting { rule } => rule,
            State::Pending { rule, .. } => {
                // A transient maximize/fullscreen during settling must cancel
                // placement even if the window is restored before the timer.
                if readiness(rule) == Readiness::Never {
                    self.set_state(hwnd, State::Done);
                }
                return None;
            }
            _ => return None,
        };
        match readiness(rule) {
            Readiness::NotYet => None,
            Readiness::Never => {
                self.set_state(hwnd, State::Done);
                None
            }
            Readiness::Ready => {
                let token = self.token();
                self.set_state(
                    hwnd,
                    State::Pending {
                        rule,
                        token,
                        attempt: 0,
                    },
                );
                Some(Schedule {
                    token,
                    delay_ms: SETTLE_MS,
                })
            }
        }
    }

    /// A one-shot timer fired. `current_pid` is the window's owner right now
    /// (`None` if the window is gone); identity is revalidated before `place`.
    pub fn on_timer(
        &mut self,
        hwnd: isize,
        token: u64,
        current_pid: Option<u32>,
        readiness: impl FnOnce(usize) -> Readiness,
        place: impl FnOnce(usize) -> PlaceOutcome,
    ) -> Option<Schedule> {
        let entry = self.entries.get(&hwnd)?;
        let State::Pending {
            rule,
            token: expected,
            attempt,
        } = entry.state
        else {
            return None;
        };
        if expected != token {
            return None;
        }
        if current_pid != Some(entry.pid) {
            // Destroyed, or HWND recycled by another process: forget it.
            self.entries.remove(&hwnd);
            return None;
        }
        match readiness(rule) {
            Readiness::NotYet => {
                self.set_state(hwnd, State::Waiting { rule });
                return None;
            }
            Readiness::Never => {
                self.set_state(hwnd, State::Done);
                return None;
            }
            Readiness::Ready => {}
        }
        match place(rule) {
            PlaceOutcome::Transient if attempt + 1 < MAX_ATTEMPTS => {
                let token = self.token();
                self.set_state(
                    hwnd,
                    State::Pending {
                        rule,
                        token,
                        attempt: attempt + 1,
                    },
                );
                Some(Schedule {
                    token,
                    delay_ms: RETRY_MS,
                })
            }
            _ => {
                self.set_state(hwnd, State::Done);
                None
            }
        }
    }

    /// Respect a user's move/resize gesture, including during initial settling.
    pub fn protect(&mut self, hwnd: isize) {
        self.set_state(hwnd, State::Done);
    }

    /// Lightweight debug state; does not inspect other processes.
    pub fn status(&self, hwnd: isize) -> &'static str {
        match self.entries.get(&hwnd).map(|e| e.state) {
            None => "new",
            Some(State::Ignored) => "unsupported",
            Some(State::Waiting { .. }) => "waiting",
            Some(State::Pending { .. }) => "pending",
            Some(State::Done) => "done (placed or protected)",
        }
    }

    /// The window was destroyed; any pending timer becomes a no-op.
    pub fn on_destroy(&mut self, hwnd: isize) {
        self.entries.remove(&hwnd);
    }

    /// Drop entries whose windows no longer exist (missed DESTROY events).
    pub fn prune(&mut self, mut alive: impl FnMut(isize, u32) -> bool) {
        self.entries.retain(|&hwnd, e| alive(hwnd, e.pid));
    }

    fn set_state(&mut self, hwnd: isize, state: State) {
        if let Some(e) = self.entries.get_mut(&hwnd) {
            e.state = state;
        }
    }

    #[cfg(test)]
    fn state(&self, hwnd: isize) -> Option<State> {
        self.entries.get(&hwnd).map(|e| e.state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    const H: isize = 0x1234;
    const PID: u32 = 42;
    const RULE: usize = 1;

    fn supported() -> Option<usize> {
        Some(RULE)
    }

    fn ready(_: usize) -> Readiness {
        Readiness::Ready
    }

    fn schedule_ready(t: &mut Tracker) -> u64 {
        t.on_window_event(H, PID, supported, ready).unwrap().token
    }

    #[test]
    fn unsupported_window_is_classified_once_and_ignored() {
        let mut t = Tracker::new();
        let calls = Cell::new(0);
        for _ in 0..5 {
            let s = t.on_window_event(
                H,
                PID,
                || {
                    calls.set(calls.get() + 1);
                    None
                },
                |_| panic!("readiness must not be checked for unsupported windows"),
            );
            assert_eq!(s, None);
        }
        assert_eq!(calls.get(), 1);
        assert_eq!(t.state(H), Some(State::Ignored));
    }

    #[test]
    fn ready_window_is_placed_exactly_once() {
        let mut t = Tracker::new();
        let s = t.on_window_event(H, PID, supported, ready).unwrap();
        assert_eq!(s.delay_ms, SETTLE_MS);
        // Repeated SHOW/NAMECHANGE while pending does not double-schedule.
        assert_eq!(t.on_window_event(H, PID, supported, ready), None);

        let placed = Cell::new(0);
        let r = t.on_timer(H, s.token, Some(PID), ready, |rule| {
            assert_eq!(rule, RULE);
            placed.set(placed.get() + 1);
            PlaceOutcome::Placed
        });
        assert_eq!(r, None);
        assert_eq!(placed.get(), 1);
        assert_eq!(t.state(H), Some(State::Done));

        // Later events (user moves, title changes, hide/show) never re-place.
        assert_eq!(t.on_window_event(H, PID, supported, ready), None);
        assert_eq!(
            t.on_timer(H, s.token, Some(PID), ready, |_| panic!("no re-place")),
            None
        );
    }

    #[test]
    fn untitled_window_becomes_ready_later() {
        let mut t = Tracker::new();
        assert_eq!(
            t.on_window_event(H, PID, supported, |_| Readiness::NotYet),
            None
        );
        assert_eq!(t.state(H), Some(State::Waiting { rule: RULE }));
        // NAMECHANGE arrives once the title is set.
        assert!(t.on_window_event(H, PID, supported, ready).is_some());
    }

    #[test]
    fn maximized_or_non_main_window_is_left_alone_for_good() {
        let mut t = Tracker::new();
        assert_eq!(
            t.on_window_event(H, PID, supported, |_| Readiness::Never),
            None
        );
        assert_eq!(t.state(H), Some(State::Done));
        assert_eq!(t.on_window_event(H, PID, supported, ready), None);
    }

    #[test]
    fn window_hidden_again_before_timer_returns_to_waiting() {
        let mut t = Tracker::new();
        let tok = schedule_ready(&mut t);
        let r = t.on_timer(H, tok, Some(PID), |_| Readiness::NotYet, |_| panic!());
        assert_eq!(r, None);
        assert_eq!(t.state(H), Some(State::Waiting { rule: RULE }));
        // Old token is now stale.
        assert_eq!(t.on_timer(H, tok, Some(PID), ready, |_| panic!()), None);
        assert!(t.on_window_event(H, PID, supported, ready).is_some());
    }

    #[test]
    fn destroyed_window_timer_is_a_no_op() {
        let mut t = Tracker::new();
        let tok = schedule_ready(&mut t);
        t.on_destroy(H);
        assert!(t.is_empty());
        assert_eq!(t.on_timer(H, tok, None, ready, |_| panic!()), None);
    }

    #[test]
    fn recycled_handle_in_same_process_rejects_old_timer() {
        let mut t = Tracker::new();
        let old = schedule_ready(&mut t);
        t.on_destroy(H);
        let new = schedule_ready(&mut t);
        assert_ne!(old, new);
        assert_eq!(
            t.on_timer(H, old, Some(PID), ready, |_| panic!("stale timer")),
            None
        );
        assert_eq!(
            t.on_timer(H, new, Some(PID), ready, |_| PlaceOutcome::Placed),
            None
        );
        assert_eq!(t.state(H), Some(State::Done));
    }

    #[test]
    fn window_gone_at_timer_is_forgotten() {
        let mut t = Tracker::new();
        let tok = schedule_ready(&mut t);
        assert_eq!(t.on_timer(H, tok, None, ready, |_| panic!()), None);
        assert!(t.is_empty());
    }

    #[test]
    fn hwnd_reused_by_other_process_before_timer_is_not_moved() {
        let mut t = Tracker::new();
        let tok = schedule_ready(&mut t);
        // Destroy event missed; same HWND value now belongs to PID 99.
        assert_eq!(t.on_timer(H, tok, Some(99), ready, |_| panic!()), None);
        assert!(t.is_empty());
    }

    #[test]
    fn hwnd_reuse_after_done_is_treated_as_new_window() {
        let mut t = Tracker::new();
        let tok = schedule_ready(&mut t);
        t.on_timer(H, tok, Some(PID), ready, |_| PlaceOutcome::Placed);
        assert_eq!(t.state(H), Some(State::Done));
        // Different PID on the same HWND: reclassified, eligible again.
        let s = t.on_window_event(H, 7, supported, ready);
        assert!(s.is_some());
        // And an unsupported reuse is ignored.
        t.on_destroy(H);
        assert_eq!(t.on_window_event(H, 8, || None, ready), None);
        assert_eq!(t.state(H), Some(State::Ignored));
    }

    #[test]
    fn transient_failures_retry_a_bounded_number_of_times() {
        let mut t = Tracker::new();
        let mut tok = schedule_ready(&mut t);
        let attempts = Cell::new(0);
        loop {
            let r = t.on_timer(H, tok, Some(PID), ready, |_| {
                attempts.set(attempts.get() + 1);
                PlaceOutcome::Transient
            });
            match r {
                Some(s) => {
                    assert_eq!(s.delay_ms, RETRY_MS);
                    tok = s.token;
                }
                None => break,
            }
        }
        assert_eq!(attempts.get(), MAX_ATTEMPTS);
        assert_eq!(t.state(H), Some(State::Done));
    }

    #[test]
    fn retry_success_stops_retrying() {
        let mut t = Tracker::new();
        let tok = schedule_ready(&mut t);
        let s = t
            .on_timer(H, tok, Some(PID), ready, |_| PlaceOutcome::Transient)
            .unwrap();
        assert_eq!(
            t.on_timer(H, s.token, Some(PID), ready, |_| PlaceOutcome::Placed),
            None
        );
        assert_eq!(t.state(H), Some(State::Done));
    }

    #[test]
    fn hard_failure_does_not_retry() {
        let mut t = Tracker::new();
        let tok = schedule_ready(&mut t);
        assert_eq!(
            t.on_timer(H, tok, Some(PID), ready, |_| PlaceOutcome::Failed),
            None
        );
        assert_eq!(t.state(H), Some(State::Done));
    }

    #[test]
    fn prune_drops_dead_windows() {
        let mut t = Tracker::new();
        t.on_window_event(1, 1, || None, ready);
        t.on_window_event(2, 2, || None, ready);
        t.prune(|hwnd, _| hwnd == 2);
        assert_eq!(t.len(), 1);
        assert_eq!(t.state(2), Some(State::Ignored));
    }
}

#[cfg(test)]
mod safety_tests {
    use super::*;
    #[test]
    fn protected_state_during_settling_cancels_stale_timer() {
        let mut t = Tracker::new();
        let s = t
            .on_window_event(1, 2, || Some(0), |_| Readiness::Ready)
            .unwrap();
        t.on_window_event(1, 2, || unreachable!(), |_| Readiness::Never);
        t.on_window_event(1, 2, || unreachable!(), |_| Readiness::Ready);
        assert_eq!(
            t.on_timer(
                1,
                s.token,
                Some(2),
                |_| Readiness::Ready,
                |_| panic!("protected window moved")
            ),
            None
        );
    }
    #[test]
    fn manual_gesture_during_settling_cancels_placement() {
        let mut t = Tracker::new();
        let s = t
            .on_window_event(1, 2, || Some(0), |_| Readiness::Ready)
            .unwrap();
        t.protect(1);
        t.on_timer(
            1,
            s.token,
            Some(2),
            |_| Readiness::Ready,
            |_| panic!("manual move overwritten"),
        );
        assert_eq!(t.status(1), "done (placed or protected)");
    }
}
