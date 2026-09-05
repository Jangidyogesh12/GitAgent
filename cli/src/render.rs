//! ============================================================================
//! Module: cli::render (src/render.rs)
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   Stream renderer: drains one SDK message channel to the terminal and
//!   reports an exit code. It subscribes to the `SdkMessage` stream (the
//!   Observer end) and formats each variant for human reading.
//!
//! HOW IT WORKS:
//!   * Loop: `rx.recv().await` until the sender closes; `code` starts at 0
//!     and flips to 1 the first time an `Error` arrives.
//!   * `Delta(text)` prints the fragment inline with no newline (streaming
//!     effect via stdout flush); the follow-up `Assistant(_)` prints the
//!     terminating newline (text was already streamed).
//!   * `ToolUse(_, name, args)` announces `⚙ name(<60-char arg preview>)`;
//!     `ToolResult(_, name, content, is_error)` prints `✓/✗ name:
//!     <200-char trimmed preview>` (tool errors stay model-visible data and
//!     do not flip the exit code by themselves).
//!   * `System(s)` prints a dim parenthetical line; `Error(e)` prints to
//!     stderr and sets exit 1. A trailing newline is printed on close.
//!   * `preview()` collapses whitespace to single spaces, then truncates to
//!     `n` chars plus `…` for the arg/result previews above.
//!
//! FUNCTIONS PRESENT IN THIS FILE:
//!   * `render_stream()` — drain the channel, print, return exit code.
//!   * `preview()`       — truncate display strings with ellipsis.
//!
//! HOW TO USE (example):
//! ```rust,no_run
//! // let code = crate::render::render_stream(&mut rx).await; // 0 ok, 1 error
//! ```
//! ============================================================================

use sdk::SdkMessage;
use tokio::sync::mpsc;

/// Drain an SDK stream to stdout; 0 = ok, 1 = error seen.
///
/// # Description
/// Streams deltas to stdout inline; `Assistant` → newline (deltas already
/// printed the text, so this only terminates the line); `ToolUse` →
/// preview (`✗` when error); `System` → dim parenthetical; `Error` → stderr
/// + exit 1.
///
/// # Example
/// ```rust,no_run
/// // async fn demo(mut rx: tokio::sync::mpsc::UnboundedReceiver<sdk::SdkMessage>) {
/// //     let code = crate::render::render_stream(&mut rx).await;
/// // }
/// ```
pub async fn render_stream(rx: &mut mpsc::UnboundedReceiver<SdkMessage>) -> i32 {
    use std::io::Write;
    let mut code = 0;
    while let Some(msg) = rx.recv().await {
        match msg {
            SdkMessage::Delta(t) => {
                print!("{t}");
                std::io::stdout().flush().ok();
            }
            SdkMessage::Assistant(_) => println!(),
            SdkMessage::ToolUse(_, name, args) => {
                println!("\n⚙ {name}({})", preview(&args.to_string(), 60));
            }
            SdkMessage::ToolResult(_, name, content, is_error) => {
                let mark = if is_error { "✗" } else { "✓" };
                println!("{mark} {name}: {}", preview(content.trim(), 200));
                if is_error {
                    // Errors are model-visible data; don't fail the shell.
                }
            }
            SdkMessage::System(s) => println!("({s})"),
            SdkMessage::Error(e) => {
                eprintln!("error: {e}");
                code = 1;
            }
        }
    }
    println!();
    code
}

/// Truncate `s` to `n` chars + `…` (display previews).
///
/// # Example
/// ```rust
/// // assert_eq!(crate::render::preview("abcdef", 3), "abc…");
/// ```
pub fn preview(s: &str, n: usize) -> String {
    let flat: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= n {
        flat
    } else {
        format!("{}…", flat.chars().take(n).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_truncates() {
        assert_eq!(preview("abcdef", 3), "abc…");
        assert_eq!(preview("ab", 5), "ab");
    }
}
