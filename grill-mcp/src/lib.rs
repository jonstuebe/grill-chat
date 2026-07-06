//! grill-mcp — voice transport for the grilling skill (ticket 0002).
//!
//! - [`engine`] is the proven Stage A voice pipeline (TTS -> mic -> VAD -> STT).
//! - `bin/roundtrip` is the standalone harness that measures the latency gate.
//! - `main.rs` wraps [`engine`] in an `rmcp` stdio MCP server (begin/ask/end).

pub mod engine;
