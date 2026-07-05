<!-- label: wayfinder:map -->

# Map: Voice-Driven Grilling — local CLI + skill, no Solo

## Destination

A validated architecture and spec for a **voice-grilling skill + local CLI** that works from any MCP-capable agent CLI (Claude Code, Gemini CLI, opencode, …), with the agent↔voice **transport proven by a working prototype**. The end of this map is a spec ready to hand to an implementing agent — not a shipped product.

## Notes

**Domain:** Replace the typed back-and-forth of text grilling skills (`grill-me`, `wayfinder`, etc.) with a live voice conversation, while all reasoning — deciding the next question from the last answer — stays inside the user's agent-CLI session. The voice side is a thin, stateless audio interface.

**Skills every session should consult:** `/grilling` and `/domain-modeling` for design tickets; `/prototype` for prototype tickets; `/research` for research tickets.

**Settled architecture (from the charting grill) — treat as fixed unless a ticket explicitly reopens it:**

1. **Transport = an MCP server** exposing a blocking call-and-response `ask` (question out → finalized transcript in). Direct replacement for the old Solo MCP bridge; natively supported by all three target CLIs.
2. **Code Mode (Cloudflare/Anthropic "code execution with MCP" pattern) compresses plumbing + tool-schema context only.** Each `ask` transcript returns to the **main agent context**, where the LLM reasons and forms the next question — that reasoning boundary is the whole point and must stay in the main session. Native MCP tool calls are the **fallback** for CLIs lacking code-exec. Running the whole loop in-sandbox is a **non-goal** (see Fog).
3. **Stack = one pure-Rust single binary.** `cpal` (mic) + `rodio` (playback), Silero VAD + `smart-turn-v3` ONNX endpointing (~8 MB, ~12 ms CPU, audio-based, no Python), local STT (`sherpa-onnx` / `whisper-cpp-plus-rs` / `parakeet-rs`), local TTS (`Kokoros` / `sherpa-onnx`), MCP server via `rmcp`. Metal-accelerated. No Python, no Node, no Docker. The `Handy` macOS app is live proof this Rust stack ships.
4. **Interaction is half-duplex, turn-based** (`Speaking → Listening → Deciding → Done`) — a ~200-line state machine we own, not a framework. Pipecat/LiveKit solve full-duplex problems we don't have; no Rust equivalent exists and none is needed.
5. **Lifecycle = lazy in-process, CLI-scoped.** Host CLI spawns the binary as its stdio MCP server; models lazy-load on the first `ask`, then stay warm for the CLI session's lifetime. No launchd/systemd service to manage.

**Key research findings backing the above:**
- Docker is disqualified on macOS: no host-audio passthrough (LinuxKit VM, no CoreAudio/`/dev/snd`) and no Metal passthrough. Confirmed by `docker/for-mac#6789` (a Whisper-in-container case that failed). Docker is a Linux-only story.
- If Python were ever needed, `uv`-bootstrap or PyInstaller/Nuitka (native, keeps CoreAudio + Metal) — but the native Rust stack removes the need.

## Decisions so far

<!-- index — one line per closed ticket, then zoom the link for detail -->

- [Design the MCP ask/session contract](tickets/0001-mcp-ask-contract.md) — three tools (`begin?` / blocking-`ask`-with-progress-heartbeats / `end`) over one implicit session, no IDs; `answer = {transcript, status, confidence, duration_ms, detail?}`; abort via native MCP cancellation; binary reports mechanical facts, skill interprets all meaning.
- [Design the voice-grill skill loop and summary](tickets/0005-voice-grill-skill.md) — `voice-grill` is a **voice mode of wayfinder** (not standalone, not a grill-me wrapper): grilling dialogue is spoken, tracker/commits stay textual; each turn speaks the recommendation as a proposal while the terminal shows full options; phase-bounded with no turn cap, one charting checkpoint, and always-flush-to-tickets on stop.

## Fog

- **Persistent OS-level daemon** (launchd/systemd, always-warm across CLI restarts) — revisit only if repeated cold-start model loads prove annoying in practice.
- **Code Mode absorbing the reasoning loop** (full ask→decide→ask in-sandbox via a sub-LLM) — explicit non-goal; revisit only if per-turn latency/context proves unacceptable.
- **Edge-case hardening beyond turn detection** (Section 7 of the original doc): false interruptions / barge-in recovery, low-confidence / garbled transcription handling, session hang & timeout fallback, extracting signal from long rambling answers, sequencing follow-ups when one answer surfaces multiple ambiguities, recognizing early-exit intent ("that's enough") as a control signal not an answer. Most are skill-reasoning concerns that firm up once the skill loop and a working prototype exist; graduate to tickets then.
- **Linux / Windows support** — macOS-first. Linux could later use Docker (native audio + NVIDIA GPU work there); Windows patterns like macOS (native bundle).
- **Auto-spawn / one-step startup** — skill spins up the binary automatically rather than relying on host MCP registration (original doc Phase 5 stretch).
