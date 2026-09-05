//! ============================================================================
//! Integration test: mock SSE server → full `complete_with_fallback`.
//! ----------------------------------------------------------------------------
//! WHAT THIS FILE IS FOR:
//!   Regression backbone: spins a
//!   tiny TCP mock speaking OpenAI SSE and drives the REAL client code, so
//!   provider parsing is covered without API keys or the network.
//!
//! HOW TO RUN: `cargo test -p llm` (or `--workspace`).
//! ============================================================================

use engine::agent::client::GenParams;
use engine::llm::{complete_with_fallback, resolve_model};
use std::io::{Read, Write};

/// Serve ONE canned SSE body per connection, `n` connections max.
fn mock_server(body_events: Vec<String>, max_conn: usize) -> (String, std::thread::JoinHandle<()>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let handle = std::thread::spawn(move || {
        for _ in 0..max_conn {
            let Ok((mut sock, _)) = listener.accept() else {
                break;
            };
            sock.set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .ok();
            // Read the HTTP request (headers + body) before replying.
            let mut buf = vec![0u8; 65536];
            let mut got = 0;
            let mut header_end = None;
            let mut content_len = 0usize;
            while header_end.is_none() && got < buf.len() {
                match sock.read(&mut buf[got..]) {
                    Ok(0) => break,
                    Ok(n) => {
                        got += n;
                        let s = String::from_utf8_lossy(&buf[..got]).into_owned();
                        if let Some(p) = s.find("\r\n\r\n") {
                            header_end = Some(p + 4);
                            for line in s[..p].lines() {
                                if let Some(v) = line.strip_prefix("Content-Length:") {
                                    content_len = v.trim().parse().unwrap_or(0);
                                }
                                if let Some(v) = line.strip_prefix("content-length:") {
                                    content_len = v.trim().parse().unwrap_or(0);
                                }
                            }
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            // Drain the body bytes so the client never blocks on send.
            let mut remaining =
                content_len as isize - (got as isize - header_end.unwrap_or(got) as isize);
            while remaining > 0 {
                let mut tmp = vec![0u8; 8192];
                match sock.read(&mut tmp[..remaining.min(8192) as usize]) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => remaining -= n as isize,
                }
            }
            let sse = format!("data: {}\n\ndata: [DONE]\n\n", body_events[0]);
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                sse.len(),
                sse
            );
            sock.write_all(resp.as_bytes()).ok();
            sock.flush().ok();
        }
    });
    (format!("http://127.0.0.1:{port}/v1"), handle)
}

#[tokio::test]
async fn text_reply_comes_through() {
    let (base, h) = mock_server(
        vec![
            r#"{"choices":[{"delta":{"content":"hello world"},"finish_reason":"stop"}]}"#
                .to_string(),
        ],
        4,
    );
    let http = reqwest::Client::new();
    let spec = resolve_model(&format!("openai:t@{base}"));
    let msg = complete_with_fallback(
        &http,
        &[spec],
        "sys",
        &[],
        &[],
        &GenParams::default(),
        0,
        "test-session",
    )
    .await;
    eprintln!("TEXT GOT: {msg:?}");
    assert_eq!(msg.text(), "hello world");
    drop(h);
}

#[tokio::test]
async fn tool_call_reply_comes_through() {
    let (base, h) = mock_server(
        vec![r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"c1","type":"function","function":{"name":"read","arguments":"{\"path\":\"a\"}"}}]},"finish_reason":"tool_calls"}]}"#.to_string()],
        4,
    );
    let http = reqwest::Client::new();
    let spec = resolve_model(&format!("openai:t@{base}"));
    let msg = complete_with_fallback(
        &http,
        &[spec],
        "sys",
        &[],
        &[],
        &GenParams::default(),
        0,
        "test-session",
    )
    .await;
    eprintln!("TOOL GOT: {msg:?}");
    assert_eq!(msg.tool_calls().len(), 1);
    drop(h);
}
