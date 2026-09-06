//! ============================================================================
//! Module: cli::spinner (src/spinner.rs)
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   Classic single-line spinner animation for long CLI operations (update
//!   downloads, uninstall). Braille-dot frames cycle next to the message
//!   while work runs in the foreground; the animation lives on a
//!   background thread and cleans up after itself.
//!
//! HOW IT WORKS:
//!   * Frames: `frame(step)` returns one braille-dot frame (pure function,
//!     unit-tested).
//!   * `Spinner::start(msg)` spawns the animation thread when stdout is a
//!     TTY (otherwise prints a plain line); `finish()`/`fail()` stop it,
//!     erase the line, and print the `✓`/`✗` outcome. `Drop` restores the
//!     cursor if the spinner is abandoned.
//!
//! FUNCTIONS PRESENT IN THIS FILE:
//!   * `frame()` — return one spinner frame (pure).
//!   * `Spinner::start()` — begin animating with a message.
//!   * `Spinner::finish()` — stop with success.
//!   * `Spinner::fail()` — stop with failure.
//!
//! HOW TO USE (examples):
//! ```rust,ignore
//! let spinner = Spinner::start("Downloading gitagent");
//! // ... do the work ...
//! spinner.finish("downloaded gitagent");
//! ```
//! ============================================================================

use std::io::{IsTerminal, Write};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread::JoinHandle;
use std::time::Duration;

/// Spinner frames (braille dots, classic rotation).
const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
/// Frame interval.
const TICK: Duration = Duration::from_millis(80);

/// Return one animation frame.
///
/// # Description
/// Pure function of `step`: frames cycle, so
/// `frame(n) == frame(n + FRAMES.len())`.
///
/// # Example
/// ```rust,ignore
/// assert_eq!(frame(0), "⠋");
/// ```
pub fn frame(step: usize) -> &'static str {
    FRAMES[step % FRAMES.len()]
}

/// Running spinner animation (see module docs).
pub struct Spinner {
    stop: Arc<AtomicBool>,
    drew: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Spinner {
    /// Begin animating with `message` followed by the spinner.
    ///
    /// # Description
    /// When stdout is not a TTY, prints one plain line and animates
    /// nothing (piped output stays clean).
    pub fn start(message: &str) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let drew = Arc::new(AtomicBool::new(false));
        let handle = if std::io::stdout().is_terminal() {
            let stop = stop.clone();
            let drew = drew.clone();
            let msg = message.to_string();
            Some(std::thread::spawn(move || {
                print!("\x1b[?25l");
                let _ = std::io::stdout().flush();
                let mut step = 0usize;
                while !stop.load(Ordering::Relaxed) {
                    print!("\r{msg} {}", frame(step));
                    let _ = std::io::stdout().flush();
                    drew.store(true, Ordering::Relaxed);
                    step += 1;
                    std::thread::sleep(TICK);
                }
            }))
        } else {
            println!("{message} …");
            None
        };
        Self { stop, drew, handle }
    }

    /// Stop the animation with success and print `✓ {msg}`.
    pub fn finish(mut self, msg: &str) {
        self.halt();
        if self.drew.load(Ordering::Relaxed) {
            print!("\r\x1b[K");
        }
        println!("✓ {msg}");
        print!("\x1b[?25h");
        let _ = std::io::stdout().flush();
    }

    /// Stop the animation with failure and print `✗ {msg}`.
    pub fn fail(mut self, msg: &str) {
        self.halt();
        if self.drew.load(Ordering::Relaxed) {
            print!("\r\x1b[K");
        }
        println!("✗ {msg}");
        print!("\x1b[?25h");
        let _ = std::io::stdout().flush();
    }

    /// Signal the thread to stop and wait for it (shared by finish/fail).
    fn halt(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for Spinner {
    /// Restore the cursor if the spinner is abandoned without finish/fail.
    fn drop(&mut self) {
        if self.handle.is_some() {
            self.halt();
            print!("\x1b[?25h");
            let _ = std::io::stdout().flush();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_cycle_through_ten_distinct_steps() {
        assert_eq!(FRAMES.len(), 10);
        assert_eq!(frame(0), "⠋");
        assert_eq!(frame(9), "⠏");
        assert_eq!(frame(10), frame(0));
        assert_ne!(frame(0), frame(1));
    }
}
