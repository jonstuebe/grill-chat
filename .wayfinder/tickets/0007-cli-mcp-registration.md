---
id: "0007"
title: Map MCP-server registration across target CLIs
type: wayfinder:research
status: open
assignee:
blocked_by: []
---

## Question

How does each target CLI register and launch a local stdio MCP server, and which of them support code-execution-of-MCP (the Code Mode path) versus needing the native-tool-call fallback?

- **Claude Code:** MCP server registration (`.mcp.json` / settings), and its code-execution-with-MCP support (executor pattern) — the primary Code Mode target.
- **Gemini CLI:** MCP config format; does it have any code-exec-of-MCP affordance, or fallback-only?
- **opencode:** same two questions.

Additionally, per the resolved contract (0001), verify for each CLI: support for **MCP progress notifications** (the `ask` heartbeat / listening indicator depends on them) and **MCP request cancellation** (the abort mechanism depends on it), plus the actual **request-timeout tolerance** for a long-blocking tool call. If any CLI lacks progress notifications or cancellation, note the degraded behavior (e.g. shorter max-answer cap, or `end()`-based abort fallback).

Also capture: how a skill ships *alongside* a binary across these CLIs, and the first-run model-download UX expectations. This decides how much of the code-mode surface is portable vs Claude-Code-first, and it shapes both the prototype (0002) and the code-mode ticket (0006).

Resolves into a research summary (linked asset) covering registration + code-exec support per CLI.
