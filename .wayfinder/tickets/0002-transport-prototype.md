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

**Stage A (standalone voice round-trip) built — `grill-mcp/src/main.rs`.** Proven live: models load fast (Kokoro 264 ms, Parakeet ~480 ms, Silero VAD 9 ms), TTS synth + playback works (question spoke aloud). Weights downloaded to `grill-mcp/models/` (~740 MB on disk). Endpointing = trailing-silence VAD (`SmoothedVad`); `smart-turn-v3` deferred to 0004.

**Full standalone round-trip PROVEN LIVE** — run from Ghostty (a mic-permitted terminal), the binary spoke the question, captured the mic, endpointed on VAD silence, and returned a correct transcript. (Latency numbers: TODO — paste from the run.)

**Mic-permission note (macOS TCC):** the capture leg fails under host apps that don't surface the mic prompt to child processes (e.g. super.engineering: auth `.notDetermined`, device visible but unopenable) — **not a code bug**. Works from a normal terminal that can prompt:
```
cd grill-mcp && ./target/release/grill-mcp   # approve the mic prompt, then speak
```

**Findings so far:**
- **TTS engine swapped** (mid-0002): the `kokoro-tts` mzdk100 int8 fork sounded unnaturally English. Replaced with the original **`kokoros`** crate (lucasjinreal git, `ort` rc.12 — alignment preserved) + **fp32** `kokoro-v1.0.onnx`, matching the `yap` project. Confirmed much more natural; default voice `af_heart` (override via `GRILL_VOICE`). Corrects the 0003 model-stack decision (crate + precision). GPL note now realized: `espeak-rs-sys` (espeak-ng) is statically linked.
- TTS is non-streaming → "text→first audio" was ~2.1 s, over the 400 ms gate; a streaming path is the fix (tuning, 0004).
- Mic-permission provisioning by host app is a real packaging concern (see map Fog; feeds 0006).

**Remaining (resume here):**
1. Complete the **live capture→transcript** leg in a mic-permitted terminal; record latency numbers.
2. `rmcp` stdio server exposing `begin` / `ask` / `end` (contract 0001).
3. Half-duplex state machine (`Speaking → Listening → Deciding → Done`).
4. First-run weight download (~740 MB) into a cache dir (currently manual into `models/`).
5. Register the binary in `.mcp.json` and drive it from a stock Claude Code session.
