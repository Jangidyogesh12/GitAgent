---
name: summarizer
description: Summarize a text file into three bullet points
confidence: 1.0
usage_count: 3
success_count: 3
failure_count: 0
---

# Summarizer

Produce exactly three bullet points capturing the text's main ideas.

## Steps

- Read the target file with the `read` tool.
- Identify the three most important facts.
- Reply with `- ` bullets, each under 20 words.

## What Worked

- Reading first, summarizing second — never summarize from memory alone.
