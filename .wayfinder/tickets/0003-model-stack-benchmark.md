---
id: "0003"
title: Choose and benchmark the local model stack
type: wayfinder:prototype
status: open
assignee:
blocked_by: []
---

## Question

Which concrete models and crates does the Rust binary use, and do they hit an acceptable latency/quality bar on Apple Silicon?

- **STT:** `sherpa-onnx` vs `whisper-cpp-plus-rs` (streaming + built-in VAD) vs `parakeet-rs` (Parakeet TDT, ~300–400 ms/utterance in the shipping `Handy` app). Pick one; measure real utterance latency on an M-series Mac with Metal/CoreML.
- **TTS:** `Kokoros` (Kokoro-82M, most natural) vs `sherpa-onnx` TTS (one dependency for STT+TTS+VAD) vs Piper (faster, more robotic). Judge naturalness for an interview voice and real-time factor.
- **VAD + turn model:** confirm Silero VAD + `smart-turn-v3` ONNX load and run from Rust `ort`; measure the feature-preprocessing cost.
- **Packaging:** which weights are bundled in the binary vs downloaded on first run, and total footprint.

Resolves into a chosen model stack + a benchmark note (linked asset). Feeds the prototype (0002). Consider whether `sherpa-onnx` as a single STT+TTS+VAD dependency simplifies enough to prefer it even at some quality cost.
