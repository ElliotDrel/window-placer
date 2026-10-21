//! Physical-pixel placement relative to the current monitor, clamped to its
//! work area. Windows scaling is deliberately not applied to the stored rules.
//! A per-monitor-aware caller sees physical coordinates even at 125% or 200%.

use crate::rules::PlacementRect;

/// Physical screen rectangle with exclusive right/bottom, like Win32 `RECT`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScreenRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl ScreenRect {
    pub fn width(&self) -> i32 {
        self.right - self.left
    }

    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }

    /// True when `self` covers all of `other` (used for fullscreen detection).
    pub fn covers(&self, other: &ScreenRect) -> bool {
        self.left <= other.left
            && self.top <= other.top
            && self.right >= other.right
            && self.bottom >= other.bottom
    }
}

/// Compute where a window should go on the monitor it currently occupies.
///
/// `monitor` is the full monitor rectangle, `work` its work area (excluding
/// the taskbar and docked app bars).
pub fn target_rect(rule: PlacementRect, monitor: ScreenRect, work: ScreenRect) -> ScreenRect {
    let width = rule.width.clamp(1, work.width().max(1));
    let height = rule.height.clamp(1, work.height().max(1));
    let left =
        (monitor.left.saturating_add(rule.x)).clamp(work.left, (work.right - width).max(work.left));
    let top =
        (monitor.top.saturating_add(rule.y)).clamp(work.top, (work.bottom - height).max(work.top));
    ScreenRect {
        left,
        top,
        right: left + width,
        bottom: top + height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(left: i32, top: i32, right: i32, bottom: i32) -> ScreenRect {
        ScreenRect {
            left,
            top,
            right,
            bottom,
        }
    }

    const CODE: PlacementRect = PlacementRect {
        x: 69,
        y: 56,
        width: 1782,
        height: 963,
    };

    #[test]
    fn full_hd_matches_physical_rule_at_any_scaling() {
        let mon = r(0, 0, 1920, 1080);
        let work = r(0, 0, 1920, 1032);
        assert_eq!(target_rect(CODE, mon, work), r(69, 56, 69 + 1782, 56 + 963));
    }

    #[test]
    fn larger_display_keeps_physical_pixel_dimensions() {
        // Windows scaling does not enlarge the physical-pixel placement rules.
        let mon = r(0, 0, 3840, 2160);
        let work = r(0, 0, 3840, 2064);
        assert_eq!(target_rect(CODE, mon, work), r(69, 56, 69 + 1782, 56 + 963));
    }

    #[test]
    fn small_display_is_clamped_to_work_area() {
        let mon = r(0, 0, 1366, 768);
        let work = r(0, 0, 1366, 728);
        let t = target_rect(CODE, mon, work);
        assert_eq!(t, r(0, 0, 1366, 728));
        assert!(work.covers(&t));
    }

    #[test]
    fn partially_too_big_shifts_back_inside() {
        let mon = r(0, 0, 1800, 1100);
        let work = r(0, 0, 1800, 1060);
        let t = target_rect(CODE, mon, work);
        assert_eq!(t.width(), 1782);
        assert_eq!(t.right, 1800);
        assert_eq!(t.top, 56);
    }

    #[test]
    fn secondary_monitor_with_negative_origin() {
        let mon = r(-1920, -200, 0, 880);
        let work = r(-1920, -200, 0, 840);
        let t = target_rect(CODE, mon, work);
        assert_eq!(
            t,
            r(-1920 + 69, -200 + 56, -1920 + 69 + 1782, -200 + 56 + 963)
        );
        assert!(work.covers(&t));
    }

    #[test]
    fn top_taskbar_pushes_window_down() {
        let mon = r(0, 0, 1920, 1080);
        let work = r(0, 60, 1920, 1080);
        let t = target_rect(CODE, mon, work);
        assert_eq!(t.top, 60);
        assert_eq!(t.height(), 963);
    }

    #[test]
    fn degenerate_inputs_do_not_panic() {
        let t = target_rect(CODE, r(0, 0, 0, 0), r(0, 0, 0, 0));
        assert_eq!((t.width(), t.height()), (1, 1));
    }
}
