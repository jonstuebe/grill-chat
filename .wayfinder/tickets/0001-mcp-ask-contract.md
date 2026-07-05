---
id: "0001"
title: Design the MCP ask/session contract
type: wayfinder:grilling
status: closed
assignee: jonstuebe
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

## Resolution

The contract is **three MCP tools over a single implicit session** (one mic, one user — no session IDs). All conversation state is skill-side; the binary reports mechanical facts only and interprets no meaning.

### Tools

- **`begin(opening?: string)`** — optional. Starts the session; if `opening` is given, speaks it (framing sentence) before any question. Auto-invoked by the first `ask` if not called explicitly.
- **`ask(question: string, opts?: { silence_timeout_ms?, max_answer_ms? }) → answer`** — the workhorse. Speaks `question` (TTS), listens, detects end-of-turn, returns the finalized transcript. **Blocking** (one call = one answer) but emits **MCP progress notifications** while listening — these keep the client's request timeout from firing *and* serve as the "🎙️ listening…" indicator. Timing has sensible baked-in defaults, optionally overridable per call.
- **`end()`** — normal teardown: stop listening/speaking, release the audio device. An **idle-timeout auto-`end`** fires as a safety net if the skill (or a crashed agent) never calls it.

### `answer` payload

```
answer = {
  transcript:   string,
  status:       "answered" | "silence_timeout" | "no_speech" | "aborted" | "error",
  confidence:   number,      // 0–1, STT-derived; carried but NOT acted on by the binary
  duration_ms:  number,
  detail?:      string       // human-readable; set on "error" / "aborted"
}
```

### Semantics / division of labor

- **Blocking + heartbeats** (not poll, not naive-blocking): keeps the clean one-call-one-answer shape that code-mode and the skill loop want, while defusing client request-timeout death on long answers.
- **Abort = native MCP request cancellation** of an in-flight `ask` (skill bails / user walks away). No dedicated `abort` tool — the surface stays exactly three verbs. Binary-originated failures surface via `status: "aborted" | "error"`.
- **Smart/dumb split:** binary reports mechanical facts (`status`, `confidence`, `duration_ms`). The skill (main context) interprets *all* meaning — stop-intent, low-confidence re-ask vs proceed-and-flag, garbled/nonsensical answers. `stop_intent` was deliberately **dropped** from the payload to avoid duplicating judgment in the dumb layer.

### Dependencies created

Per-CLI support for **MCP progress notifications** and **request cancellation**, plus exact request-timeout tolerances, must be verified in the CLI-registration research (0007). The low-confidence and stop-intent *policies* (not the mechanism) belong to the skill design (0005).

