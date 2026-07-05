---
id: "0003"
title: Choose and benchmark the local model stack
type: wayfinder:prototype
status: closed
assignee: jonstuebe
blocked_by: []
---

## Question

Which concrete models and crates does the Rust binary use, and do they hit an acceptable latency/quality bar on Apple Silicon?

- **STT:** `sherpa-onnx` vs `whisper-cpp-plus-rs` (streaming + built-in VAD) vs `parakeet-rs` (Parakeet TDT, ~300–400 ms/utterance in the shipping `Handy` app). Pick one; measure real utterance latency on an M-series Mac with Metal/CoreML.
- **TTS:** `Kokoros` (Kokoro-82M, most natural) vs `sherpa-onnx` TTS (one dependency for STT+TTS+VAD) vs Piper (faster, more robotic). Judge naturalness for an interview voice and real-time factor.
- **VAD + turn model:** confirm Silero VAD + `smart-turn-v3` ONNX load and run from Rust `ort`; measure the feature-preprocessing cost.
- **Packaging:** which weights are bundled in the binary vs downloaded on first run, and total footprint.

Resolves into a chosen model stack + a benchmark note (linked asset). Feeds the prototype (0002). Consider whether `sherpa-onnx` as a single STT+TTS+VAD dependency simplifies enough to prefer it even at some quality cost.

## Resolution

Full note: **[Model stack decision + evidence](../assets/0003-model-stack.md)** (chosen crates/models, corrections, footprint, latency bar, risks, citations).

**Approach (user decision):** evidence-based pick now; no standalone benchmark harness. Real latency becomes the **first acceptance gate of the prototype (0002)** — the one place that has to load these models anyway.

**Chosen stack — best-of-breed on one shared `ort` runtime (user decision):**
- **STT:** `transcribe-rs` (cjpais) — Parakeet-TDT-0.6B default, Whisper+Metal fallback. (This, not `parakeet-rs`, is Handy's actual shipping path.)
- **TTS:** `Kokoros` — Kokoro-82M, ~0.1 RTF on M-series CPU, Apache-2.0.
- **VAD:** `silero-vad-rs` — tiny, CPU, MIT.
- **Turn:** `smart-turn-v3` (pipecat-ai) — self-wired via `ort` (~8 MB, BSD-2); no Rust crate wraps it yet.

**Rejected:** single `sherpa-onnx` dependency — the popular `sherpa-rs` binding was archived June 2026, and it buys no acceleration edge (would still bolt on smart-turn).

**Three corrections to the map** (folded into spine #3): (1) the stack runs **CPU-bound**, not "Metal-accelerated" — Parakeet CoreML is flaky, Kokoro needs no GPU, and that's acceptable (Metal only for the Whisper fallback); (2) Handy uses `transcribe-rs`, and the "~300–400 ms" figure was unconfirmed; (3) `sherpa-rs` is archived.

**Packaging:** ~700 MB of weights (Parakeet dominates) → **download on first run**, not bundled. Feeds 0006.

**Licensing (user decision):** Kokoro's espeak-ng g2p is GPL-3.0. Prototype uses it as-is; relicensing to permissive (swap g2p) recorded as a shipping-gate Fog item.
