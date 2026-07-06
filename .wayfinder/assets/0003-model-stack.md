# Model stack decision + evidence note

Asset for ticket 0003. Evidence-based pick (no standalone benchmark harness this session — per the resolution decision, real latency is measured as the **first acceptance gate of the transport prototype (0002)**). Verified against current repos/crates.io/release notes, July 2026.

## Chosen stack

| Component | Crate | Model | Footprint (int8 ONNX) | Accel | License (code / weights) |
|---|---|---|---|---|---|
| **STT** | `transcribe-rs` (cjpais) | Parakeet-TDT-0.6B (default), Whisper (fallback) | ~600 MB | **CPU** (`ort-coreml` flaky; whisper has real Metal) | MIT-Apache / CC-BY-4.0 |
| **TTS** | `Kokoros` (lucasjinreal) | Kokoro-82M | ~88 MB | CPU (~0.1 RTF, no GPU needed) | Apache-2.0 / Apache-2.0 (**g2p via espeak-ng = GPL-3.0**, see below) |
| **VAD** | `silero-vad-rs` | Silero VAD | ~few MB | CPU | MIT / MIT |
| **Turn** | self-wired via `ort` | `smart-turn-v3` (pipecat-ai) | ~8 MB | CPU (~12–60 ms) | — / BSD-2-Clause |

**One shared ONNX runtime (`ort`) across all four models** (best-of-breed, not a single sherpa-onnx dependency). Rationale: every piece is independently proven (`transcribe-rs` + Parakeet + Silero is Handy's exact shipping path; Kokoros has real M-series RTF data), and it avoids both the dual-ORT link risk and the just-archived `sherpa-rs` binding.

## Why each pick

- **STT — `transcribe-rs`, not `parakeet-rs` directly.** `transcribe-rs` is what the shipping Handy app actually uses (correction: the map/earlier notes said `parakeet-rs`). It's multi-engine (Parakeet default + Whisper fallback behind `whisper-metal`) and exposes an `ort-coreml` flag. Parakeet-TDT-0.6B is the quality/speed sweet spot at ~5× realtime on CPU. The Whisper fallback is our insurance if Parakeet CPU latency misses the bar — Whisper has *real, mature Metal* acceleration where Parakeet's CoreML does not.
- **TTS — standalone `Kokoros`.** Kokoro-82M is best-in-class naturalness for its size, ~0.1 RTF on M-series CPU (no GPU needed), Apache-2.0 weights. sherpa-onnx serves the *same* weights (identical quality) but we're not taking the single-dep path, so standalone Kokoros is the narrower dependency.
- **VAD — `silero-vad-rs`.** Tiny, CPU-cheap, MIT, bundles its own weights. The low-risk win; already used by Handy.
- **Turn — `smart-turn-v3`, self-wired.** Real (~8 MB int8, BSD-2), but **no Rust crate wraps it yet** — we write the `ort`-loading + Whisper-Tiny-style mel front-end ourselves. This glue is required regardless of harness choice.

## Corrections this ticket makes to the map

1. **"Metal-accelerated" is wrong as a blanket claim.** STT (Parakeet CoreML flaky — [onnxruntime#26355](https://github.com/microsoft/onnxruntime/issues/26355)), TTS (Kokoro needs no GPU), VAD, and turn all run **CPU-bound** on Apple Silicon, and that is acceptable (Parakeet ~5× RT, Kokoro ~0.1 RTF, smart-turn ~12–60 ms). Metal only matters for the *Whisper fallback*. Spine decision #3 updated.
2. **Handy uses `transcribe-rs` (cjpais), not `parakeet-rs`.** And the "~300–400 ms/utterance" figure is *unconfirmed* — Handy's actual claim is "sub-200 ms to transcription start" / Parakeet ~5× realtime CPU.
3. **`sherpa-rs` (thewh1teagle) was archived June 2026** → the official `sherpa-onnx` crate is the newer, less-proven replacement. Reinforces the best-of-breed choice.

## Packaging / footprint

- Total weights ≈ **~700 MB** (Parakeet 0.6B int8 dominates). Too large to vendor in the plugin binary.
- **Decision: download weights on first run** into a cache dir (`${CLAUDE_PLUGIN_DATA}` or XDG cache), not bundled. Binary stays small; weights fetched once, verified by checksum. Feeds ticket 0006 (plugin packaging).

## Acceptance latency bar (handed to 0002)

The prototype (0002) must measure and meet, on the target M-series Mac:

- **End-of-speech → finalized transcript ≤ ~1.0 s** (STT + turn decision).
- **TTS: question text → first audio ≤ ~400 ms.**
- **Turn detection (smart-turn-v3 inference + features) ≤ ~100 ms.**

If STT misses the bar on Parakeet/CPU → switch `transcribe-rs` to the **Whisper + Metal** engine (first fallback) before reopening 0003. If TTS misses → int8-quantize Kokoro / trim voice.

## Open risks to watch in 0002

- **`ort` version alignment** across `transcribe-rs`, `Kokoros`, `silero-vad-rs`, and our smart-turn glue — a mismatch can cause duplicate/native-lib link clashes. First thing to prove in 0002's build.
- **smart-turn-v3 Rust integration is greenfield** — mel front end + `ort` session; budget time.
- **Parakeet CoreML instability** — plan for CPU EP; Whisper-Metal is the escape hatch.
- **espeak-ng/GPL-3.0 g2p** — prototype uses it (best quality, zero friction); relicensing is a shipping-gate item (see map Fog).

## Sources

- `transcribe-rs` / Handy: https://github.com/cjpais/transcribe-rs · https://github.com/cjpais/Handy
- `parakeet-rs`: https://github.com/altunenes/parakeet-rs · weights https://huggingface.co/nvidia/parakeet-tdt-0.6b-v2 · CoreML bug https://github.com/microsoft/onnxruntime/issues/26355
- `Kokoros`: https://github.com/lucasjinreal/Kokoros · weights https://huggingface.co/hexgrad/Kokoro-82M · espeak/GPL https://github.com/hexgrad/kokoro/issues/247
- Silero VAD (Rust): https://crates.io/crates/silero-vad-rs
- `smart-turn-v3`: https://huggingface.co/pipecat-ai/smart-turn-v3 · https://www.daily.co/blog/announcing-smart-turn-v3-with-cpu-inference-in-just-12ms/
- `sherpa-onnx` (official Rust) / `sherpa-rs` archived: https://crates.io/crates/sherpa-onnx · https://github.com/thewh1teagle/sherpa-rs · Kokoro in sherpa https://k2-fsa.github.io/sherpa/onnx/tts/pretrained_models/kokoro.html
- `whisper-rs`: https://github.com/tazz4843/whisper-rs

## Correction (from 0002, prototype)

The TTS pick above named the `kokoro-tts` crate (mzdk100 fork, int8). In the prototype its English sounded unnatural. **Replaced with the original `kokoros` crate** (lucasjinreal git, pinned to yap's commit; still `ort` =2.0.0-rc.12, so alignment holds) + the **fp32** `kokoro-v1.0.onnx` (~310 MB, up from 88 MB int8) — the same stack the `yap` project uses. Confirmed much more natural. Consequences: total footprint grows (~310 MB TTS instead of ~88 MB); English g2p is `espeak-rs`/espeak-ng (statically linked, GPL-3.0 — the shipping-gate is now realized).
