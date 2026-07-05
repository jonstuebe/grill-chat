---
id: "0001"
title: Design the MCP ask/session contract
type: wayfinder:grilling
status: open
assignee:
blocked_by: []
---

## Question

What is the exact MCP contract between the agent CLI and the voice binary? Nail down the message/session shapes and their semantics:

- **start session** — optional framing/setup (e.g. a sentence to speak before the first question).
- **ask (blocking)** — question text in; does not complete until a finalized answer is available.
- **answer** — finalized transcript out, plus minimal metadata the skill's reasoning needs: transcription confidence, a `stop_intent` flag (user said "that's enough"), and error/empty signals.
- **end session** — stop listening/speaking, release resources.
- **abort / error** — either direction (mic failed, user walked away, incoherent STT) so the skill never hangs.

Also decide: per-`ask` **timeout** semantics and what a timeout returns; whether state (turn history) is server-side or entirely skill-side (settled: skill-side — confirm the contract carries none); and how the tool set is shaped so Code Mode can wrap the plumbing turns while `ask` stays a main-context boundary.

Resolves into a written contract spec (the transport half of the destination). Blocks the prototype (0002) and the skill design (0005).
