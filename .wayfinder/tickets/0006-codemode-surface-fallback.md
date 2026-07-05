---
id: "0006"
title: Code-mode surface and native-tool fallback across CLIs
type: wayfinder:prototype
status: open
assignee:
blocked_by: ["0002", "0007"]
---

## Question

How is the Code Mode surface delivered, and how does it degrade gracefully where code-exec isn't available? Design and prove:

- The **generated TS API** the agent writes code against (the plumbing turns — start/end/abort/retry — collapse into single script runs; `ask` stays a main-context boundary).
- The **fallback to native MCP tool calls** on CLIs without code-execution-of-MCP (per 0007's findings — likely Gemini/opencode).
- Verify the context savings are real (schemas load on demand; plumbing chatter doesn't accumulate) without leaking the per-turn reasoning out of main context.

Resolves into a code-mode delivery spec + a demonstration that the same skill works via code-mode on Claude Code and via native tools on a fallback CLI.
