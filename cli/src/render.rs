//! ============================================================================
//! Module: cli::render (src/render.rs)
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   Stream renderer (Observer of the SDK channel). Ports `handleEvent()`
//!   from `src/index.ts`: print text deltas inline, dim thinking (skipped —
//!   SDK surfaces text only), announce tool calls with 60-char arg previews,
//!   preview results at 200 chars, print per-turn usage, return exit code.
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
