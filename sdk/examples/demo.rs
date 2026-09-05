//! SDK demo: stream one prompt + define a custom closure tool.
//!
//! Run (offline — expects failure path without a key, or set one):
//! ```bash
//! cargo run -p sdk --example demo -- ./my-agent "hello"
//! ```

use sdk::{query, tool, QueryOptions};
use std::path::PathBuf;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| ".".to_string());
    let prompt = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "say hi".to_string());

    // Custom tool in four lines: name + description + JSON schema + closure.
    let shout = tool(
        "shout",
        "Uppercase some text",
        serde_json::json!({
            "type": "object",
            "properties": {"text": {"type": "string"}},
            "required": ["text"]
        }),
        |args| {
            Ok(args
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_uppercase())
        },
    );

    let mut opts = QueryOptions::new(PathBuf::from(dir), prompt);
    opts.extra_tools.push(Arc::new(shout));

    let mut rx = query(opts);
    while let Some(msg) = rx.recv().await {
        match msg {
            sdk::SdkMessage::Delta(t) => print!("{t}"),
            sdk::SdkMessage::Assistant(t) => println!("\n[assistant] {t}"),
            sdk::SdkMessage::ToolUse(_, name, _) => println!("\n[tool] {name}"),
            sdk::SdkMessage::ToolResult(_, name, content, err) => {
                println!(
                    "[result {} err={err}] {}",
                    name,
                    &content[..content.len().min(120)]
                )
            }
            sdk::SdkMessage::System(s) => println!("[{s}]"),
            sdk::SdkMessage::Error(e) => eprintln!("error: {e}"),
        }
    }
}
