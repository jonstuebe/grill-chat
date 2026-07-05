---
id: "0002"
title: Transport round-trip prototype
type: wayfinder:prototype
status: open
assignee:
blocked_by: ["0001", "0003"]
---

## Question

Does the core round trip work end to end? Prove: a **stock Claude Code session spawns the Rust binary as its MCP server**, calls `ask("<question>")`, the binary **speaks the question** (TTS), **listens** and **detects end-of-turn**, and **returns the finalized transcript** back into the session — one turn, no looping logic yet.

This is the destination's required proof-of-transport. Minimal is fine: hardcode the model choice from 0003 and a minimal contract from 0001, single question, in-process lazy model load on first `ask`.

Resolves when the one-turn round trip is demonstrably working (link a short recording/log as an asset). Unblocks turn-detection hardening (0004) and the code-mode/fallback work (0006).
