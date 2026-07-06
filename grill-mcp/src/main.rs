//! grill-mcp — the transport (ticket 0002, Stage B): an `rmcp` stdio MCP server
//! that wraps the proven [`grill_mcp::engine`] voice pipeline behind the 3-tool
//! contract from ticket 0001 (`begin` / `ask` / `end`) over one implicit
//! session (one mic, one user, no session IDs). All conversation state stays
//! with the caller; this binary reports mechanical facts only.
//!
//! Architecture: the voice models + microphone live on a single dedicated
//! **worker thread** (its own current-thread runtime, used only to `block_on`
//! the async model load). This keeps the `!Send`/`!Sync` audio types (cpal
//! stream, ort sessions) on one thread and serializes the half-duplex session
//! naturally. The async MCP handlers send a command down a channel, await a
//! reply, and — for the blocking `ask` — emit MCP progress notifications on a
//! ticker so the client's request timeout never fires while we listen.
//!
//! stdout is the JSON-RPC channel; ALL diagnostics go to stderr.

use anyhow::Result;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::model::{
    ProgressNotificationParam, ProgressToken, ServerCapabilities, ServerInfo,
};
use rmcp::service::{RequestContext, RoleServer};
use rmcp::transport::stdio;
use rmcp::{tool, tool_handler, tool_router, ErrorData, ServerHandler, ServiceExt};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};

use grill_mcp::engine::{Answer, AnswerStatus, ListenCfg, VoiceEngine};

/// Commands sent from the async MCP layer to the voice-worker thread.
enum Cmd {
    Begin {
        opening: Option<String>,
        reply: oneshot::Sender<std::result::Result<bool, String>>, // Ok(spoke?)
    },
    Ask {
        question: String,
        cfg: ListenCfg,
        reply: oneshot::Sender<Answer>,
    },
    End {
        reply: oneshot::Sender<()>,
    },
}

/// Spawn the dedicated voice-worker thread and return its command sender.
/// Models lazy-load on the first `begin`/`ask` and then stay warm.
fn spawn_worker() -> mpsc::UnboundedSender<Cmd> {
    let (tx, mut rx) = mpsc::unbounded_channel::<Cmd>();
    std::thread::Builder::new()
        .name("voice-worker".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("build voice-worker runtime");
            let mut engine: Option<VoiceEngine> = None;

            while let Some(cmd) = rx.blocking_recv() {
                match cmd {
                    Cmd::Begin { opening, reply } => {
                        let r = (|| -> Result<bool> {
                            let eng = ensure_loaded(&mut engine, &rt)?;
                            match opening.as_deref() {
                                Some(text) => {
                                    eng.speak(text)?;
                                    Ok(true)
                                }
                                None => Ok(false),
                            }
                        })();
                        let _ = reply.send(r.map_err(|e| e.to_string()));
                    }
                    Cmd::Ask { question, cfg, reply } => {
                        let ans = match ensure_loaded(&mut engine, &rt) {
                            Ok(eng) => eng.ask_turn(&question, cfg),
                            Err(e) => Answer {
                                transcript: String::new(),
                                status: AnswerStatus::Error,
                                confidence: 0.0,
                                duration_ms: 0,
                                detail: Some(e.to_string()),
                            },
                        };
                        let _ = reply.send(ans);
                    }
                    Cmd::End { reply } => {
                        // Models stay warm for reuse; the mic is already dropped
                        // at the end of every `listen`, so there's nothing else
                        // to release. This is the contract's teardown ack.
                        let _ = reply.send(());
                    }
                }
            }
            eprintln!("[worker] command channel closed; voice-worker exiting");
        })
        .expect("spawn voice-worker thread");
    tx
}

/// Load the models on first use (blocking on the worker's own runtime).
fn ensure_loaded<'a>(
    engine: &'a mut Option<VoiceEngine>,
    rt: &tokio::runtime::Runtime,
) -> Result<&'a mut VoiceEngine> {
    if engine.is_none() {
        eprintln!("[worker] loading models (first use)…");
        let (e, _timings) = rt.block_on(VoiceEngine::load())?;
        *engine = Some(e);
    }
    Ok(engine.as_mut().expect("engine loaded"))
}

// ---------- MCP tool argument / result shapes ----------

#[derive(Debug, Deserialize, JsonSchema)]
struct BeginArgs {
    /// Optional framing sentence to speak aloud before any question.
    #[serde(default)]
    opening: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct AskArgs {
    /// The question to speak aloud, then listen for an answer to.
    question: String,
    /// Give up with `no_speech` if the user never starts speaking within this
    /// many milliseconds (default 8000).
    #[serde(default)]
    silence_timeout_ms: Option<u64>,
    /// Hard cap on a single answer in milliseconds (default 30000).
    #[serde(default)]
    max_answer_ms: Option<u64>,
}

#[derive(Debug, Serialize, JsonSchema)]
struct BeginAck {
    ok: bool,
    /// Whether an opening line was spoken.
    spoke: bool,
}

#[derive(Debug, Serialize, JsonSchema)]
struct EndAck {
    ok: bool,
}

/// The `answer` payload from contract 0001.
#[derive(Debug, Serialize, JsonSchema)]
struct AskAnswer {
    transcript: String,
    /// "answered" | "no_speech" | "error"
    status: String,
    /// 0–1, STT-derived; carried but not acted on by the binary.
    confidence: f64,
    /// Captured answer audio length, in ms.
    duration_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
}

impl From<Answer> for AskAnswer {
    fn from(a: Answer) -> Self {
        Self {
            transcript: a.transcript,
            status: a.status.as_str().to_string(),
            confidence: a.confidence as f64,
            duration_ms: a.duration_ms,
            detail: a.detail,
        }
    }
}

// ---------- The server ----------

#[derive(Clone)]
struct GrillServer {
    tx: mpsc::UnboundedSender<Cmd>,
    tool_router: ToolRouter<GrillServer>,
}

impl GrillServer {
    fn new(tx: mpsc::UnboundedSender<Cmd>) -> Self {
        Self {
            tx,
            tool_router: Self::tool_router(),
        }
    }

    fn worker_gone() -> ErrorData {
        ErrorData::internal_error("voice worker is not running", None)
    }
    fn no_reply() -> ErrorData {
        ErrorData::internal_error("voice worker dropped the reply", None)
    }
}

#[tool_router]
impl GrillServer {
    #[tool(
        description = "Begin the voice session. If `opening` is given, speak it aloud as a framing line before any question. Optional — the first `ask` auto-begins the session."
    )]
    async fn begin(
        &self,
        Parameters(args): Parameters<BeginArgs>,
    ) -> std::result::Result<Json<BeginAck>, ErrorData> {
        let (rtx, rrx) = oneshot::channel();
        self.tx
            .send(Cmd::Begin {
                opening: args.opening,
                reply: rtx,
            })
            .map_err(|_| Self::worker_gone())?;
        let spoke = rrx
            .await
            .map_err(|_| Self::no_reply())?
            .map_err(|e| ErrorData::internal_error(e, None))?;
        Ok(Json(BeginAck { ok: true, spoke }))
    }

    #[tool(
        description = "Speak `question` aloud, listen to the microphone, detect end-of-turn (trailing-silence VAD), and return the finalized transcript. Blocking — one call yields one answer — and emits progress notifications while listening so the request never times out."
    )]
    async fn ask(
        &self,
        Parameters(args): Parameters<AskArgs>,
        ctx: RequestContext<RoleServer>,
    ) -> std::result::Result<Json<AskAnswer>, ErrorData> {
        let defaults = ListenCfg::default();
        let cfg = ListenCfg {
            end_silence_ms: defaults.end_silence_ms,
            initial_silence_ms: args.silence_timeout_ms.unwrap_or(defaults.initial_silence_ms),
            max_answer_ms: args.max_answer_ms.unwrap_or(defaults.max_answer_ms),
        };

        let (rtx, mut rrx) = oneshot::channel::<Answer>();
        self.tx
            .send(Cmd::Ask {
                question: args.question,
                cfg,
                reply: rtx,
            })
            .map_err(|_| Self::worker_gone())?;

        // Heartbeat: keep the client's request timeout alive and drive the
        // "🎙️ listening…" indicator while the worker blocks on the mic.
        let token: Option<ProgressToken> = ctx.meta.get_progress_token();
        let mut ticker = tokio::time::interval(std::time::Duration::from_millis(1500));
        ticker.tick().await; // consume the immediate first tick
        let mut progress = 0.0f64;

        let answer = loop {
            tokio::select! {
                r = &mut rrx => break r.map_err(|_| Self::no_reply())?,
                _ = ticker.tick() => {
                    progress += 1.0;
                    if let Some(t) = token.clone() {
                        let _ = ctx.peer.notify_progress(
                            ProgressNotificationParam::new(t, progress).with_message("listening…"),
                        ).await;
                    }
                }
            }
        };
        Ok(Json(AskAnswer::from(answer)))
    }

    #[tool(
        description = "End the voice session: stop listening/speaking and release the audio device. Safety-net teardown; the models stay warm for the process lifetime."
    )]
    async fn end(&self) -> std::result::Result<Json<EndAck>, ErrorData> {
        let (rtx, rrx) = oneshot::channel();
        self.tx
            .send(Cmd::End { reply: rtx })
            .map_err(|_| Self::worker_gone())?;
        rrx.await.map_err(|_| Self::no_reply())?;
        Ok(Json(EndAck { ok: true }))
    }
}

#[tool_handler]
impl ServerHandler for GrillServer {
    fn get_info(&self) -> ServerInfo {
        // ServerInfo / Implementation are #[non_exhaustive]; mutate a default
        // rather than constructing with a struct literal.
        let mut info = ServerInfo::default();
        info.capabilities = ServerCapabilities::builder().enable_tools().build();
        info.server_info.name = "grill-mcp".into();
        info.server_info.version = env!("CARGO_PKG_VERSION").into();
        info.instructions = Some(
            "Voice transport for grilling. begin(opening?) speaks a framing line; \
             ask(question) speaks it, listens, and returns the transcript; end() releases \
             the mic. One implicit session (one mic, one user); all conversation state stays \
             with the caller."
                .into(),
        );
        info
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Warn early (on stderr) if weights are missing — before we touch stdio.
    if let Err(e) = VoiceEngine::check_artifacts() {
        eprintln!("[grill-mcp] warning: {e}");
    }

    let tx = spawn_worker();
    let server = GrillServer::new(tx);
    eprintln!("[grill-mcp] serving MCP over stdio (tools: begin, ask, end)…");

    let service = server.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
