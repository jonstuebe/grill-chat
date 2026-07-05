# ort alignment finding — best-of-breed on one runtime (proven)

Asset for ticket 0002. Result of the ticket's mandated first step: prove the model crates resolve to and link against **one** `ort` / onnxruntime, with no duplicate-symbol clash. **Verdict: proven** (`cargo build` exit 0, binary runs). Verified July 2026 with Rust 1.94 / Cargo 1.93.

## The conflict (real, as flagged)

Naive "latest of every crate" does **not** resolve — `ort` is pre-release (`2.0.0-rc.N`), and Cargo does not unify different pre-releases:

| Crate | role | `ort` requirement |
|---|---|---|
| `transcribe-rs` 0.3.11 | STT (Parakeet) + Silero VAD | `=2.0.0-rc.12` |
| `kokoro-tts` 0.3.3 | TTS (Kokoro-82M) | `^2.0.0-rc.12` → rc.12 |
| `silero-vad-rs` 0.1.2 | standalone VAD | `=2.0.0-rc.9` ❌ |
| `voice_activity_detector` 0.2.1 | standalone VAD | `=2.0.0-rc.10` ❌ |

The STT and TTS crates already agree on **rc.12**; only the standalone VAD crates pin older, incompatible `ort`.

## The fix (best-of-breed survives)

`transcribe-rs` exposes a **`vad-silero`** feature that ships Silero VAD through *its own* `ort` — so we drop the standalone VAD crate entirely and take VAD from the STT crate on the same runtime. `smart-turn-v3` stays self-wired via our direct `ort` dep (still no Rust crate wraps it).

Resulting aligned set — all on one `ort =2.0.0-rc.12`:

```toml
transcribe-rs = { version = "0.3.11", features = ["onnx", "vad-silero"] }  # STT + Silero VAD
kokoro-tts    = "0.3.3"                                                     # TTS
ort           = "=2.0.0-rc.12"                                             # smart-turn-v3 glue
```

`cargo tree -i ort` → single `ort v2.0.0-rc.12` + single `ort-sys v2.0.0-rc.12`. Build: exit 0, one `ort-sys` compiled, 433 KB binary runs.

## Corrections this surfaces

- **VAD is not a separate crate.** The map's spine #3 named `silero-vad-rs`; it pins an incompatible `ort` and is dropped. VAD comes from `transcribe-rs` `vad-silero`.
- **TTS crate is `kokoro-tts` (crates.io), not the `Kokoros` git repo** named in 0003. Same Kokoro-82M model; `kokoro-tts` is the packaged crate and aligns on rc.12.
- **onnxruntime is statically linked** (`cargo:rustc-link-lib=static=onnxruntime`, ort-sys `download-binaries` default) → self-contained binary, good for plugin packaging (0006).

## Feature notes for the build-out

- STT engine runs **CPU EP** by default (`onnx`); `ort-coreml` is opt-in (Parakeet CoreML flaky — stay CPU).
- **Whisper+Metal fallback** = add feature `whisper-metal` (pulls `whisper-rs`, compiles whisper.cpp — heavier build; not enabled in the spike).
- Pins are exact (`=rc.12`) — deliberate; do not bump `ort` casually or the alignment breaks again.

## Still open (needs live audio hardware — the user)

The alignment/link risk is retired, but 0002 is **not resolved**. Remaining: wire `rmcp` stdio server + the `begin`/`ask`/`end` contract, the half-duplex state machine, first-run weight download, and the **live speak→listen→transcript round trip** measured against the latency gate (≤1.0 s speech→transcript, ≤400 ms TTS first-audio, ≤100 ms turn). That round trip requires a mic and speakers.
