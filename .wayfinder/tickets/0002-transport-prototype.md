---
id: "0002"
title: Transport round-trip prototype
type: wayfinder:prototype
status: claimed
assignee: jonstuebe
blocked_by: ["0001", "0003"]
---

## Question

Does the core round trip work end to end? Prove: a **stock Claude Code session spawns the Rust binary as its MCP server**, calls `ask("<question>")`, the binary **speaks the question** (TTS), **listens** and **detects end-of-turn**, and **returns the finalized transcript** back into the session — one turn, no looping logic yet.

This is the destination's required proof-of-transport. Minimal is fine: hardcode the [model choice from 0003](../assets/0003-model-stack.md) and a minimal contract from 0001, single question, in-process lazy model load on first `ask`.

**First acceptance gate (from 0003):** measure real latency on the target M-series Mac and meet — end-of-speech → finalized transcript ≤ ~1.0 s; TTS text → first audio ≤ ~400 ms; turn detection ≤ ~100 ms. The stack runs **CPU-bound** (Parakeet CoreML is flaky); if STT misses the bar, switch `transcribe-rs` to the **Whisper+Metal** engine before reopening 0003. **Prove `ort` version alignment** across `transcribe-rs` / `Kokoros` / `silero-vad-rs` / the self-wired `smart-turn-v3` glue as the first build step — a mismatch risks a native-lib link clash.

Resolves when the one-turn round trip is demonstrably working and the latency gate is met (link a short recording/log + the latency numbers as an asset). Unblocks turn-detection hardening (0004) and plugin packaging (0006).

## Progress (not resolved)

**Alignment/link risk retired** — see [ort alignment finding](../assets/0002-ort-alignment.md). The `grill-mcp/` crate is scaffolded and **builds (exit 0) against a single `ort =2.0.0-rc.12`** with a statically-linked onnxruntime. Key correction: the standalone VAD crates pin an incompatible older `ort`, so VAD comes from `transcribe-rs`'s `vad-silero` feature instead; TTS crate is `kokoro-tts`. Aligned deps:
```toml
transcribe-rs = { version = "0.3.11", features = ["onnx", "vad-silero"] }
kokoro-tts    = "0.3.3"
ort           = "=2.0.0-rc.12"
```

**Remaining (needs a live mic + speakers — resume here):**
1. `rmcp` stdio server exposing `begin` / `ask` / `end` (contract 0001).
2. Half-duplex state machine (`Speaking → Listening → Deciding → Done`).
3. First-run weight download (~700 MB) into a cache dir.
4. Self-wire `smart-turn-v3` (mel front end + `ort` session) for end-of-turn.
5. **Live round trip** + measure against the latency gate. Register the built binary in `.mcp.json` and drive it from a stock Claude Code session.
