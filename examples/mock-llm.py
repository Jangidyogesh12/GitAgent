#!/usr/bin/env python3
r"""Mock OpenAI-compatible SSE server for offline end-to-end tests.

Serves a fixed two-turn script on 127.0.0.1:8090:
  turn 1 → a `read` tool call for SOUL.md (proves tool execution),
  turn 2 → final text (proves the loop feeds results back).

Usage:
    python3 examples/mock-llm.py &
    OPENAI_API_KEY=dummy cargo run -p cli -- --dir examples/demo-agent \\
        --model "openai:mock@http://127.0.0.1:8090/v1" -p "read the soul file"
    # expect: banner → tool call → tool result → final text → (session_end)
    kill %1
"""

import json
import sys
from http.server import BaseHTTPRequestHandler, HTTPServer
from typing import ClassVar

if sys.version_info >= (3, 12):
    from typing import override
else:
    from typing_extensions import override


HOST = "127.0.0.1"
PORT = 8090


def _tool_call_event() -> str:
    """One SSE data payload requesting a `read` tool call.

    NOTE: `finish_reason` lives INSIDE the choice object — the valid
    OpenAI shape. (A past bug put it outside; the Rust client rightly
    rejected that payload, so keep this shape intact.)
    """
    return json.dumps(
        {
            "choices": [
                {
                    "delta": {
                        "tool_calls": [
                            {
                                "index": 0,
                                "id": "c1",
                                "type": "function",
                                "function": {
                                    "name": "read",
                                    "arguments": json.dumps({"path": "SOUL.md"}),
                                },
                            }
                        ]
                    },
                    "finish_reason": "tool_calls",
                }
            ]
        }
    )


def _text_event(text: str) -> str:
    """One SSE data payload carrying final answer text."""
    return json.dumps(
        {"choices": [{"delta": {"content": text}, "finish_reason": "stop"}]}
    )


# Fixed script: first connection gets the tool call, every later one
# gets the final answer (mirrors a two-turn agent run).
EVENTS: list[str] = [
    _tool_call_event(),
    _text_event("Soul file loaded and understood."),
]


class Handler(BaseHTTPRequestHandler):
    """Single-request handler serving the next scripted SSE turn."""

    count: ClassVar[int] = 0

    def do_POST(self) -> None:
        """Read the chat request, reply with the next scripted SSE event."""
        length = int(self.headers.get("Content-Length", "0"))
        # Drain the request body (fixed script ignores it); `_ =` marks the
        # discarded bytes return as intentional (strict reportUnusedCallResult).
        _ = self.rfile.read(length)
        body = EVENTS[min(Handler.count, len(EVENTS) - 1)]
        Handler.count += 1
        sse = f"data: {body}\n\ndata: [DONE]\n\n".encode()
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Content-Length", str(len(sse)))
        self.end_headers()
        # Write length is irrelevant to a mock; `_ =` marks it intentional.
        _ = self.wfile.write(sse)

    @override
    def log_message(self, format: str, *args: object) -> None:
        """Silence stdlib per-request logging (override, intentionally quiet)."""


def main() -> None:
    """Serve the mock LLM forever (Ctrl-C to stop)."""
    print(f"mock LLM on http://{HOST}:{PORT} (Ctrl-C to stop)")
    HTTPServer((HOST, PORT), Handler).serve_forever()


if __name__ == "__main__":
    main()
