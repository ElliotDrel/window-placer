//! window-placer: place supported app main windows once, when they are ready.
//!
//! * `rules`    – editable app rules (exe, optional title filter, rectangle)
//! * `geometry` – pure physical-pixel work-area clamping
//! * `tracker`  – pure per-window lifecycle (once-only, bounded retry, reuse)
//! * `win`      – the only module with `unsafe`; Win32 wrappers and hooks
//! * `app`      – CLI parsing, logging and the message loop

#![cfg(windows)]

pub mod app;
pub mod geometry;
pub mod rules;
pub mod tracker;
pub mod win;

pub mod diagnostics;
