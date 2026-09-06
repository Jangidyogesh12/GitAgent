//! ============================================================================
//! Module: cli::spinner (src/spinner.rs)
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   Square-spiral progress animation for long CLI operations (update
//!   downloads, uninstall). A block travels a 5x3 clockwise-inward square
//!   spiral while work runs in the foreground; the animation lives on a
//!   background thread and cleans up after itself.
//!
//! HOW IT WORKS:
//!   * Frames: `frame(step)` renders the 5x3 grid with `█` on the spiral
//!     head and `·` elsewhere (pure function, unit-tested).
//!   * `Spinner::start(msg)` spawns the animation thread when stdout is a
//!     TTY (otherwise prints a plain line); `finish()`/`fail()` stop it,
//!     erase the grid, and print the `✓`/`✗` outcome. `Drop` restores the
//!     cursor if the spinner is abandoned.
//!
//! FUNCTIONS PRESENT IN THIS FILE:
//!   * `frame()` — render one spiral step (pure).
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

/// Grid width (5x3 reads as a square given terminal cell aspect ratio).
const WIDTH: usize = 5;
/// Grid height.
const HEIGHT: usize = 3;
/// Frame interval.
const TICK: Duration = Duration::from_millis(80);

/// Square-spiral path: clockwise from the top-left, winding inward.
const PATH: [(usize, usize); 15] = [
    (0, 0),
    (1, 0),
    (2, 0),
    (3, 0),
    (4, 0),
    (4, 1),
    (4, 2),
    (3, 2),
    (2, 2),
    (1, 2),
    (0, 2),
    (0, 1),
    (1, 1),
    (2, 1),
    (3, 1),
];

/// Render one animation step as a 3-line grid.
///
/// # Description
/// Pure function of `step`: the spiral head is `█`, every other cell `·`.
/// Steps wrap around `PATH`, so `frame(n) == frame(n + PATH.len())`.
///
/// # Example
/// ```rust,ignore
/// assert_eq!(frame(0), "█····\n·····\n·····");
/// ```
pub fn frame(step: usize) -> String {
    let head = PATH[step % PATH.len()];
    let mut out = String::with_capacity((WIDTH + 1) * HEIGHT);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            out.push(if (x, y) == head { '█' } else { '·' });
        }
        if y + 1 < HEIGHT {
            out.push('\n');
        }
    }
    out
}

/// Running square-spiral animation (see module docs).
pub struct Spinner {
    stop: Arc<AtomicBool>,
    drew: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Spinner {
    /// Begin animating with `message` on the line above the spiral.
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
                    if step > 0 {
                        print!("\x1b[{HEIGHT}A\r");
                    }
                    print!("{msg} …\n{}", frame(step));
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
            print!("\x1b[{}A\r\x1b[J", HEIGHT + 1);
        }
        println!("✓ {msg}");
        print!("\x1b[?25h");
        let _ = std::io::stdout().flush();
    }

    /// Stop the animation with failure and print `✗ {msg}`.
    pub fn fail(mut self, msg: &str) {
        self.halt();
        if self.drew.load(Ordering::Relaxed) {
            print!("\x1b[{}A\r\x1b[J", HEIGHT + 1);
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
    use std::collections::HashSet;

    #[test]
    fn path_stays_in_bounds_and_visits_unique_cells() {
        let cells: HashSet<(usize, usize)> = PATH.into_iter().collect();
        assert_eq!(cells.len(), PATH.len());
        for (x, y) in PATH {
            assert!(x < WIDTH && y < HEIGHT);
        }
    }

    #[test]
    fn frames_render_head_on_spiral() {
        assert_eq!(frame(0), "█····\n·····\n·····");
        assert_eq!(frame(5), "·····\n····█\n·····");
        assert_eq!(frame(14), "·····\n···█·\n·····");
        assert_eq!(PATH.len(), 15);
    }

    #[test]
    fn frames_wrap_and_advance() {
        assert_eq!(frame(15), frame(0));
        assert_ne!(frame(0), frame(1));
    }
}
