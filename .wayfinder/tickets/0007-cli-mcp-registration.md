---
id: "0007"
title: Map MCP-server registration across target CLIs
type: wayfinder:research
status: closed
assignee: jonstuebe
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

## Resolution

Full findings: **[CLI MCP support matrix](../assets/0007-cli-mcp-support.md)** (all three CLIs, with config snippets and citations).

Two decisive findings:

1. **Code mode is unavailable in *every* CLI.** Model-written code calling MCP tools in a sandbox is an Anthropic API / Agent SDK pattern, not a feature of Claude Code, opencode, or Gemini CLI. All three do **native per-tool calls**. Claude Code's context-reduction mechanism is **MCP Tool Search / `alwaysLoad`**, not code execution. → Reverses spine decision #2; reframes ticket 0006.
2. **Blocking-`ask` viability is per-CLI:** Claude Code works **natively** (stdio exempt from idle timeout, ~28 h per-tool default); opencode needs progress heartbeats + ≥ v1.17.8; Gemini has a ~60 s hard wall with progress/cancellation broken.

**Decision (user, this session): focus on Claude Code only for now.** Multi-CLI support (opencode is close via `.claude/skills/` compatibility; Gemini is degraded) is deferred to Fog. Consequences:

- The blocking `ask` needs **no timeout workaround** on Claude Code stdio — the sub-60 s cap and heartbeat-to-survive-timeout concerns are multi-CLI issues, now deferred. Progress notifications are still worth emitting for the rendered "listening" indicator.
- **Cancellation stays defensive:** handle `notifications/cancelled` AND stdin-close / broken-pipe (ESC cancels the call without killing the stdio server, but a clean `cancelled` isn't doc-guaranteed).
- **Packaging path:** ship as a **Claude Code plugin** (skill + `.mcp.json` + bundled binary via `${CLAUDE_PLUGIN_ROOT}`, `alwaysLoad: true`) — the low-friction one-install path. Reframes ticket 0006 into plugin packaging.

