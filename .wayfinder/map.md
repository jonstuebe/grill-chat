<!-- label: wayfinder:map -->

# Map: Voice-Driven Grilling — local CLI + skill, no Solo

## Destination

A validated architecture and spec for a **voice-grilling skill + local CLI** that works from **Claude Code** (multi-CLI support deferred — see Fog), with the agent↔voice **transport proven by a working prototype**. The end of this map is a spec ready to hand to an implementing agent — not a shipped product.

## Notes

**Domain:** Replace the typed back-and-forth of text grilling skills (`grill-me`, `wayfinder`, etc.) with a live voice conversation, while all reasoning — deciding the next question from the last answer — stays inside the user's agent-CLI session. The voice side is a thin, stateless audio interface.

**Skills every session should consult:** `/grilling` and `/domain-modeling` for design tickets; `/prototype` for prototype tickets; `/research` for research tickets.

**Settled architecture (from the charting grill) — treat as fixed unless a ticket explicitly reopens it:**

1. **Transport = an MCP server** exposing a blocking call-and-response `ask` (question out → finalized transcript in). Direct replacement for the old Solo MCP bridge; natively supported by all three target CLIs.
2. **~~Code Mode~~ → native per-tool calls (revised by 0007).** Research found **no CLI supports code execution with MCP** — it's an API/SDK-only pattern. All hosts do native per-tool calls. Context minimization instead comes from a **tiny 3-tool surface + Claude Code's `alwaysLoad` / MCP Tool Search**. The reasoning boundary is unchanged and central: each `ask` transcript returns to the **main agent context**, where the LLM reasons and forms the next question. Running the loop in-sandbox remains a **non-goal** (Fog).
3. **Stack = one pure-Rust single binary** (concrete models chosen by 0003; crate alignment proven by 0002). `cpal` (mic) + `rodio` (playback); **best-of-breed on one shared `ort =2.0.0-rc.12`** (statically-linked onnxruntime): STT `transcribe-rs` (Parakeet-TDT-0.6B, `onnx`+`vad-silero` features; Whisper+Metal fallback via `whisper-metal` — Handy's actual path), **VAD via that same `transcribe-rs` `vad-silero`** (the standalone `silero-vad-rs`/`voice_activity_detector` crates pin an *incompatible older* `ort` — dropped), TTS via the original **`kokoros`** crate (lucasjinreal git, `ort` rc.12; proper `espeak-rs` g2p; **fp32** `kokoro-v1.0.onnx`, ~310 MB) — the `kokoro-tts` mzdk100 int8 fork sounded unnaturally English and was replaced (0002), matching the `yap` project's stack; turn `smart-turn-v3` (self-wired via `ort`); MCP server via `rmcp`. **Runs CPU-bound**, not Metal-accelerated (revised by 0003: Parakeet CoreML is flaky, Kokoro needs no GPU — acceptable; Metal is only the Whisper fallback). Weights (~700 MB) download on first run, not bundled. No Python, no Node, no Docker. The `Handy` macOS app is live proof this Rust stack ships.
4. **Interaction is half-duplex, turn-based** (`Speaking → Listening → Deciding → Done`) — a ~200-line state machine we own, not a framework. Pipecat/LiveKit solve full-duplex problems we don't have; no Rust equivalent exists and none is needed.
5. **Lifecycle = lazy in-process, CLI-scoped.** Host CLI spawns the binary as its stdio MCP server; models lazy-load on the first `ask`, then stay warm for the CLI session's lifetime. No launchd/systemd service to manage.

**Key research findings backing the above:**
- Docker is disqualified on macOS: no host-audio passthrough (LinuxKit VM, no CoreAudio/`/dev/snd`) and no Metal passthrough. Confirmed by `docker/for-mac#6789` (a Whisper-in-container case that failed). Docker is a Linux-only story.
- If Python were ever needed, `uv`-bootstrap or PyInstaller/Nuitka (native, keeps CoreAudio + Metal) — but the native Rust stack removes the need.

## Decisions so far

<!-- index — one line per closed ticket, then zoom the link for detail -->

- [Design the MCP ask/session contract](tickets/0001-mcp-ask-contract.md) — three tools (`begin?` / blocking-`ask`-with-progress-heartbeats / `end`) over one implicit session, no IDs; `answer = {transcript, status, confidence, duration_ms, detail?}`; abort via native MCP cancellation; binary reports mechanical facts, skill interprets all meaning.
- [Design the voice-grill skill loop and summary](tickets/0005-voice-grill-skill.md) — `voice-grill` is a **voice mode of wayfinder** (not standalone, not a grill-me wrapper): grilling dialogue is spoken, tracker/commits stay textual; each turn speaks the recommendation as a proposal while the terminal shows full options; phase-bounded with no turn cap, one charting checkpoint, and always-flush-to-tickets on stop.
- [Map MCP-server registration across target CLIs](tickets/0007-cli-mcp-registration.md) — [full matrix](assets/0007-cli-mcp-support.md); **code mode is unavailable in every CLI** (native per-tool calls only) and blocking `ask` works **natively on Claude Code stdio** (idle-exempt); **scope narrowed to Claude Code**, packaged as a plugin (skill + `.mcp.json` + bundled binary, `alwaysLoad`), cancellation handled defensively.
- [Choose and benchmark the local model stack](tickets/0003-model-stack-benchmark.md) — [decision + evidence](assets/0003-model-stack.md); evidence-based pick, real latency measured as 0002's first acceptance gate. **Best-of-breed on one `ort` runtime:** STT `transcribe-rs` (Parakeet + Whisper fallback), TTS `Kokoros` (Kokoro-82M), VAD `silero-vad-rs`, turn `smart-turn-v3` (self-wired). Runs **CPU-bound** (corrects "Metal-accelerated"); weights download on first run (~700 MB); espeak-ng g2p is GPL-3.0 → relicensing is a shipping gate.
- [Transport round-trip prototype](tickets/0002-transport-prototype.md) — **proven.** Stage A crushed the latency gate live (142 ms end-of-speech→transcript, 1 ms turn; TTS first-audio 553 ms → streaming tuning). Stage B wrapped the pipeline in an `rmcp` 2.1 stdio server (`begin`/`ask`/`end`, contract 0001) on a dedicated voice-worker thread with progress-heartbeat `ask`; proven headlessly (handshake, `tools/list` schemas, clean stdout, `begin` speak-leg end-to-end through MCP) and registered in repo-root `.mcp.json`. Only the `ask` mic leg awaits a mic-permitted Claude Code host (the documented TCC wall) — the leg itself is already proven in Stage A.

## Fog

- **Multi-CLI support** — scope is now Claude Code only. **opencode** is close (reads `.claude/skills/`; needs progress heartbeats + ≥ v1.17.8). **Gemini CLI** is degraded (~60 s hard timeout, progress/cancellation broken) — would need a sub-60 s max-answer cap. Revisit after the Claude Code path is proven.
- **TTS first-audio latency** — non-streaming `synth()` measured 553 ms (over the 400 ms bar) in 0002; `kokoros` streaming API emits the first chunk in ~100 ms. Switch to streaming synth when tuning responsiveness. (STT/turn gates are already crushed: 142 ms / 1 ms.)
- **Persistent OS-level daemon** (launchd/systemd, always-warm across CLI restarts) — revisit only if repeated cold-start model loads prove annoying in practice.
- **Code Mode absorbing the reasoning loop** (full ask→decide→ask in-sandbox via a sub-LLM) — explicit non-goal; revisit only if per-turn latency/context proves unacceptable.
- **Edge-case hardening beyond turn detection** (Section 7 of the original doc): false interruptions / barge-in recovery, low-confidence / garbled transcription handling, session hang & timeout fallback, extracting signal from long rambling answers, sequencing follow-ups when one answer surfaces multiple ambiguities, recognizing early-exit intent ("that's enough") as a control signal not an answer. Most are skill-reasoning concerns that firm up once the skill loop and a working prototype exist; graduate to tickets then.
- **TTS relicensing (shipping gate)** — **now realized:** the `kokoros` TTS path statically links `espeak-rs-sys`/espeak-ng (**GPL-3.0**) for English g2p, and it's the quality difference that made the voice acceptable (0002). The distributed binary therefore inherits GPL-3.0 (as does `yap`, which is licensed GPL-3.0-or-later). If a permissive license is required for distribution, swap to a non-GPL g2p (Misaki / dictionary-based) before ship — at a likely quality cost. Decide when packaging (0006) firms up.
- **Linux / Windows support** — macOS-first. Linux could later use Docker (native audio + NVIDIA GPU work there); Windows patterns like macOS (native bundle).
- **Microphone-permission provisioning (macOS TCC)** — a voice binary's mic access is inherited from the *host app* that spawns it. Proven in 0002: under super.engineering the prompt never surfaces to child processes (auth `.notDetermined`, device unopenable), while a normal terminal prompts fine. When shipped as a Claude Code plugin, mic access depends on whichever app hosts Claude Code (Terminal/iTerm/Ghostty/VS Code/…). Needs a first-run permission-check + user guidance, and possibly a bundled helper. Feeds packaging (0006); may graduate to its own ticket.
- **Auto-spawn / one-step startup** — skill spins up the binary automatically rather than relying on host MCP registration (original doc Phase 5 stretch).
