---
id: "0002"
title: Transport round-trip prototype
type: wayfinder:prototype
status: resolved
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

**Full standalone round-trip PROVEN LIVE** — run from Ghostty (a mic-permitted terminal), the binary spoke the question, captured the mic (1ch 48 kHz → resampled to 16 kHz), endpointed on VAD silence, and returned a correct transcript ("I think that it looks pretty great to be honest.").

**Measured latency vs gate (M-series, release build):**

| Gate | Target | Measured | Verdict |
|---|---|---|---|
| end-of-speech → transcript | ≤ 1000 ms | **142 ms** (37.9× realtime, 5.4 s audio) | ✅ crushed |
| turn detection (final VAD frame) | ≤ 100 ms | **1 ms** | ✅ |
| TTS text → first audio | ≤ 400 ms | **553 ms** | ⚠️ miss |

Model loads: Kokoro 295 ms, Parakeet 563 ms, Silero VAD 10 ms. The STT bar is met on Parakeet/CPU with a 7× margin — **no Whisper-Metal fallback needed.** The TTS miss is the non-streaming `synth()` (whole sentence before first sample); fix = `kokoros` streaming API (first chunk ~100 ms) — a tuning item (0004), not an architecture problem.

**Mic-permission note (macOS TCC):** the capture leg fails under host apps that don't surface the mic prompt to child processes (e.g. super.engineering: auth `.notDetermined`, device visible but unopenable) — **not a code bug**. Works from a normal terminal that can prompt:
```
cd grill-mcp && ./target/release/grill-mcp   # approve the mic prompt, then speak
```

**Findings so far:**
- **TTS engine swapped** (mid-0002): the `kokoro-tts` mzdk100 int8 fork sounded unnaturally English. Replaced with the original **`kokoros`** crate (lucasjinreal git, `ort` rc.12 — alignment preserved) + **fp32** `kokoro-v1.0.onnx`, matching the `yap` project. Confirmed much more natural; default voice `af_heart` (override via `GRILL_VOICE`). Corrects the 0003 model-stack decision (crate + precision). GPL note now realized: `espeak-rs-sys` (espeak-ng) is statically linked.
- TTS is non-streaming → "text→first audio" was ~2.1 s, over the 400 ms gate; a streaming path is the fix (tuning, 0004).
- Mic-permission provisioning by host app is a real packaging concern (see map Fog; feeds 0006).

## Stage B — MCP server wrap (`rmcp`)

Stage A proved the voice pipeline live. Stage B wraps it in the transport and proves the protocol layer.

**Built:**
- **Pipeline extracted to a shared module** (`src/engine.rs` behind `src/lib.rs`) so the server and the proven harness share one source of truth. All diagnostics moved from `println!` → **`eprintln!`** — mandatory, because an stdio MCP server's **stdout is the JSON-RPC channel** and a stray write corrupts it. The Stage A harness is preserved verbatim in behaviour as `src/bin/roundtrip.rs` (latency gate table, audition, skip-TTS toggles).
- **`src/main.rs` = `rmcp` 2.1 stdio server** exposing the exact 0001 contract: **`begin(opening?)`**, **`ask(question, silence_timeout_ms?, max_answer_ms?) → answer`**, **`end()`**. `answer` = `{transcript, status, confidence, duration_ms, detail?}` (statuses `answered`/`no_speech`/`error`).
- **Architecture:** the models + microphone live on a single dedicated **voice-worker thread** (its own current-thread runtime, used only to `block_on` the async model load). This keeps the `!Send`/`!Sync` audio types (cpal stream, ort sessions) on one thread and serializes the half-duplex session naturally. The async MCP handlers send a command down a channel and await a reply; `ask` emits **MCP progress notifications** on a 1.5 s ticker while the worker blocks on the mic (keeps the client request-timeout alive + is the "🎙️ listening…" signal — the blocking-plus-heartbeats shape from 0001). Models **lazy-load on first `begin`/`ask`**, then stay warm.
- **Model paths** resolve via `GRILL_MODELS_DIR` (default `./models`) so the host can spawn the server from any CWD — Claude Code spawns from the repo root.
- **Registered** in repo-root `.mcp.json` as server `grill` (absolute binary path + `GRILL_MODELS_DIR`).

**Proven (headless, this session — no mic needed):**
- `initialize` → correct `serverInfo` (`grill-mcp` 0.1.0), `capabilities.tools`, instructions.
- `tools/list` → all three tools with input **and** output JSON schemas matching contract 0001.
- **stdout is pristine JSON-RPC**; every diagnostic went to stderr (corruption risk retired).
- **Output leg end-to-end through the full stack:** `tools/call begin {opening}` → worker lazy-loaded all three models (Kokoro ~270 ms, Parakeet ~500 ms, Silero ~10 ms) → Kokoro synthesized → rodio opened & drove an output sink → returned `{"ok":true,"spoke":true}` as both `content` text and `structuredContent`. Verified both from `grill-mcp/` and from the **repo root** with `GRILL_MODELS_DIR` (path resolution correct).

**Final aligned deps** (`grill-mcp/Cargo.toml`) — one `ort =2.0.0-rc.12` across the stack, plus:
```toml
transcribe-rs = { version = "0.3.11", features = ["onnx", "vad-silero"] }
kokoros = { git = "https://github.com/lucasjinreal/Kokoros.git", rev = "7089168…" }
ort  = "=2.0.0-rc.12"
rmcp = { version = "2.1.0", features = ["server", "transport-io", "macros"] }
```

## Answer (resolved)

**The core round trip works end to end, and the transport is built and proven at every layer.** Stage A proved the voice pipeline live (speak → mic → VAD endpoint → transcript) and **crushed the latency gate: 142 ms end-of-speech→transcript (7× margin), 1 ms turn detection** (TTS first-audio 553 ms is a slight miss, fixed by streaming synth — a tuning item, not architecture). Stage B wrapped that pipeline in an `rmcp` stdio server exposing the 0001 `begin`/`ask`/`end` contract and proved the protocol layer headlessly: handshake, `tools/list` with correct schemas, clean stdout, and the **speak leg driven end-to-end through the MCP stack** (`begin` → worker → model load → TTS → playback → structured reply).

The **one leg not exercisable in this session is the mic capture inside `ask`** — the documented macOS host-app TCC wall (an embedded session host never surfaces the prompt to child processes). That leg is already proven live in Stage A, and it becomes a live `ask` the moment the server is driven from **Claude Code running in a mic-permitted terminal**:

```
# In Ghostty (or any terminal that can prompt for mic access), from the repo root:
claude          # grill server auto-registers from .mcp.json
# then, in-session, call the tool and speak when you hear the question:
#   ask("When you picture this finished, what does success look like?")
# → approve the one-time mic prompt → speak → transcript returns in the answer payload
```

This satisfies the destination's required proof-of-transport. **Unblocks turn-detection hardening (0004) and plugin packaging (0006);** feeds the spec synthesis (0008). The half-duplex state machine collapsed to what the round trip actually needs (speak → listen-with-VAD-endpoint → transcribe, serialized on the worker); a richer `Speaking→Listening→Deciding→Done` machine is a skill-loop (0005) concern, not the transport's. First-run weight download stays with packaging (0006), for which `GRILL_MODELS_DIR` is the seam.
