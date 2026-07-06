//! Voice pipeline — the proven Stage A round trip (ticket 0002), extracted so
//! both the standalone harness (`bin/roundtrip`) and the MCP server (`main.rs`)
//! share one source of truth: Kokoro TTS -> rodio playback, cpal mic capture,
//! trailing-silence VAD endpointing (transcribe-rs Silero), Parakeet STT.
//!
//! IMPORTANT: every diagnostic here goes to **stderr** (`eprintln!`). The MCP
//! server speaks JSON-RPC on stdout; a stray stdout write corrupts the stream.
//!
//! Semantic end-of-turn (smart-turn-v3) is deferred to the turn-detection
//! ticket (0004); trailing-silence VAD proved sufficient for the transport
//! round trip and crushed the latency gate (142 ms end-of-speech -> transcript).

use std::num::NonZero;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use kokoros::tts::koko::TTSKoko;
use transcribe_rs::onnx::parakeet::{ParakeetModel, ParakeetParams};
use transcribe_rs::onnx::Quantization;
use transcribe_rs::vad::{SileroVad, SmoothedVad, Vad};

// Model artifact filenames (downloaded on first run — see 0006 fog). TTS uses
// the `kokoros` crate + fp32 Kokoro-v1.0 (same engine/weights as the `yap`
// project) — noticeably more natural English than the int8 mzdk100 fork.
const KOKORO_MODEL: &str = "kokoro-v1.0.onnx";
const KOKORO_VOICES: &str = "voices-v1.0.bin";
const PARAKEET_DIR: &str = "parakeet-tdt-0.6b-v3-int8";
const SILERO_VAD: &str = "silero_vad_v4.onnx";

/// Base directory holding the model artifacts. Defaults to `./models` (relative
/// to the process CWD, as the `roundtrip` harness expects), overridable with
/// `GRILL_MODELS_DIR` — required when the MCP host spawns the server from a
/// different working directory (Claude Code spawns from the repo root).
fn models_dir() -> PathBuf {
    std::env::var_os("GRILL_MODELS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("models"))
}

fn model_path(name: &str) -> PathBuf {
    models_dir().join(name)
}

const TTS_LANG: &str = "en-us";
const TTS_SPEED: f32 = 1.0;
const TTS_SR: u32 = 24_000; // Kokoro emits 24 kHz mono f32.

// Endpointing thresholds.
const VAD_SR: u32 = 16_000; // Silero VAD + Parakeet operate at 16 kHz.
const VAD_FRAME: usize = 480; // 30 ms @ 16 kHz (SileroVad::frame_size()).

/// Default endpointing config; individual fields are overridable per `ask`.
#[derive(Clone, Copy, Debug)]
pub struct ListenCfg {
    /// Trailing silence (ms) that ends a turn once speech has started.
    pub end_silence_ms: u64,
    /// Give up (no_speech) if the user never starts speaking within this (ms).
    pub initial_silence_ms: u64,
    /// Hard cap (ms) on a single answer.
    pub max_answer_ms: u64,
}

impl Default for ListenCfg {
    fn default() -> Self {
        Self {
            end_silence_ms: 800,
            initial_silence_ms: 8_000,
            max_answer_ms: 30_000,
        }
    }
}

/// Per-model load timings, surfaced for gate reporting in the harness.
#[derive(Clone, Copy, Debug, Default)]
pub struct LoadTimings {
    pub tts_ms: f64,
    pub stt_ms: f64,
    pub vad_ms: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TurnStatus {
    EndOfTurn,
    NoSpeech,
    MaxAnswer,
}

/// Result of one listen leg.
pub struct Turn {
    pub status: TurnStatus,
    pub last_vad_ms: f64, // cost of the final VAD-frame inference.
    pub audio_secs: f64,  // captured audio length at 16 kHz.
}

/// Mechanical outcome of one `ask` turn — mirrors the 0001 `answer` payload.
/// The binary reports facts only; the skill interprets all meaning.
pub enum AnswerStatus {
    Answered,
    NoSpeech,
    Error,
}

impl AnswerStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            AnswerStatus::Answered => "answered",
            AnswerStatus::NoSpeech => "no_speech",
            AnswerStatus::Error => "error",
        }
    }
}

pub struct Answer {
    pub transcript: String,
    pub status: AnswerStatus,
    pub confidence: f32,
    pub duration_ms: u64,
    pub detail: Option<String>,
}

/// Loaded voice models. Owns the mic only transiently (inside `listen`), so an
/// instance is safe to keep warm for a whole session on a single owning thread.
pub struct VoiceEngine {
    tts: TTSKoko,
    stt: ParakeetModel,
    vad: SmoothedVad,
    voice: String,
}

impl VoiceEngine {
    /// Fail early with a clear message if any weight is missing.
    pub fn check_artifacts() -> Result<()> {
        for name in [KOKORO_MODEL, KOKORO_VOICES, PARAKEET_DIR, SILERO_VAD] {
            let p = model_path(name);
            if !p.exists() {
                anyhow::bail!(
                    "missing model artifact: {} (set GRILL_MODELS_DIR or run the model download first)",
                    p.display()
                );
            }
        }
        Ok(())
    }

    /// Load all three models. Async only because `TTSKoko::new` is async.
    pub async fn load() -> Result<(Self, LoadTimings)> {
        Self::check_artifacts()?;
        let mut t = LoadTimings::default();

        let start = Instant::now();
        let kokoro_model = model_path(KOKORO_MODEL);
        let kokoro_voices = model_path(KOKORO_VOICES);
        let tts = TTSKoko::new(
            &kokoro_model.to_string_lossy(),
            &kokoro_voices.to_string_lossy(),
        )
        .await;
        t.tts_ms = ms(start);
        eprintln!("[load] Kokoro TTS      {:>7.0} ms", t.tts_ms);

        let start = Instant::now();
        let stt = ParakeetModel::load(&model_path(PARAKEET_DIR), &Quantization::Int8)?;
        t.stt_ms = ms(start);
        eprintln!("[load] Parakeet STT    {:>7.0} ms", t.stt_ms);

        let start = Instant::now();
        let silero = SileroVad::new(&model_path(SILERO_VAD), 0.3)?;
        let vad = SmoothedVad::new(Box::new(silero), 15, 15, 2);
        t.vad_ms = ms(start);
        eprintln!("[load] Silero VAD      {:>7.0} ms", t.vad_ms);

        Ok((
            Self {
                tts,
                stt,
                vad,
                voice: default_voice(),
            },
            t,
        ))
    }

    pub fn voice(&self) -> &str {
        &self.voice
    }

    pub fn set_voice(&mut self, voice: String) {
        self.voice = voice;
    }

    /// Synthesize `text` to 24 kHz mono f32 without playing it.
    /// Returns (audio, synth_ms) — synth_ms is the non-streaming "text -> first
    /// audio" cost (whole sentence before the first sample; see 0004 fog).
    pub fn synth(&self, text: &str) -> Result<(Vec<f32>, f64)> {
        let start = Instant::now();
        let audio = self
            .tts
            .tts_raw_audio(text, TTS_LANG, &self.voice, TTS_SPEED, None, None, None, None)
            .map_err(|e| anyhow::anyhow!("synth failed: {e}"))?;
        Ok((audio, ms(start)))
    }

    /// Synthesize and play `text`, blocking until playback finishes.
    /// Returns synth_ms for gate reporting.
    pub fn speak(&self, text: &str) -> Result<f64> {
        let (audio, synth_ms) = self.synth(text)?;
        play_blocking(&audio, TTS_SR)?;
        Ok(synth_ms)
    }

    /// Capture the default input device until the turn ends (trailing silence),
    /// returning 16 kHz mono f32 samples ready for Parakeet.
    pub fn listen(&mut self, cfg: ListenCfg) -> Result<(Vec<f32>, Turn)> {
        listen(&mut self.vad, cfg)
    }

    /// Transcribe 16 kHz mono samples. Returns (text, stt_ms).
    pub fn transcribe(&mut self, samples16k: &[f32]) -> Result<(String, f64)> {
        let start = Instant::now();
        let result = self.stt.transcribe_with(samples16k, &ParakeetParams::default())?;
        Ok((result.text.trim().to_string(), ms(start)))
    }

    /// One full turn for the MCP `ask` path: speak the question, listen, and
    /// transcribe. Never panics — failures come back as `AnswerStatus::Error`
    /// so the caller (and skill) never hangs.
    pub fn ask_turn(&mut self, question: &str, cfg: ListenCfg) -> Answer {
        match self.ask_turn_inner(question, cfg) {
            Ok(a) => a,
            Err(e) => Answer {
                transcript: String::new(),
                status: AnswerStatus::Error,
                confidence: 0.0,
                duration_ms: 0,
                detail: Some(e.to_string()),
            },
        }
    }

    fn ask_turn_inner(&mut self, question: &str, cfg: ListenCfg) -> Result<Answer> {
        self.speak(question)?;
        eprintln!("🎙️  listening… (speak, then pause)");
        let (samples16k, turn) = self.listen(cfg)?;
        let duration_ms = (turn.audio_secs * 1000.0) as u64;

        if turn.status == TurnStatus::NoSpeech {
            return Ok(Answer {
                transcript: String::new(),
                status: AnswerStatus::NoSpeech,
                confidence: 0.0,
                duration_ms,
                detail: Some(format!(
                    "no speech within {} ms",
                    cfg.initial_silence_ms
                )),
            });
        }

        let (transcript, _stt_ms) = self.transcribe(&samples16k)?;
        let detail = (turn.status == TurnStatus::MaxAnswer)
            .then(|| format!("capped at max_answer_ms={}", cfg.max_answer_ms));

        Ok(Answer {
            transcript,
            status: AnswerStatus::Answered,
            // Parakeet via transcribe-rs does not surface a confidence scalar;
            // the contract carries it but the binary never acts on it. 1.0 is a
            // placeholder until token-level scores are wired (skill treats it as
            // "unknown, proceed"). See 0001 / smart/dumb split.
            confidence: 1.0,
            duration_ms,
            detail,
        })
    }
}

/// Candidate interview voices to audition (kokoros voice names).
pub fn candidate_voices() -> Vec<&'static str> {
    vec![
        "af_heart", "af_bella", "af_nicole", "af_aoede", "am_michael", "am_puck", "am_fenrir",
        "bf_emma", "bm_george",
    ]
}

/// Voice selected by GRILL_VOICE, else af_heart (yap's default).
pub fn default_voice() -> String {
    std::env::var("GRILL_VOICE").unwrap_or_else(|_| "af_heart".into())
}

fn ms(since: Instant) -> f64 {
    since.elapsed().as_secs_f64() * 1000.0
}

/// Capture the default input device until the turn ends, returning 16 kHz mono.
fn listen(vad: &mut SmoothedVad, cfg: ListenCfg) -> Result<(Vec<f32>, Turn)> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| anyhow::anyhow!("no input device"))?;
    #[allow(deprecated)]
    let dev_name = device.name().unwrap_or_default();
    eprintln!("[mic] device: {dev_name:?}");

    // Raw mono f32 at the device rate, filled from the audio callback.
    let raw = Arc::new(Mutex::new(Vec::<f32>::new()));
    let err_fn = |e| eprintln!("stream error: {e}");

    // Attempt build_input_stream directly with explicit configs. This *device
    // access* is what triggers the macOS mic-permission prompt (a property
    // query like default_input_config() only errors when unauthorized). Under a
    // host app without mic entitlement (e.g. an embedded session host) these
    // opens fail with misleading "device unavailable" errors and no TCC prompt
    // — run from a terminal that can prompt. See ticket 0002 / map Fog.
    let mut chosen: Option<(cpal::Stream, u32)> = None;
    'outer: for &sr in &[48_000u32, 44_100, 16_000] {
        for &ch in &[1u16, 2] {
            let scfg = cpal::StreamConfig {
                channels: ch,
                sample_rate: sr,
                buffer_size: cpal::BufferSize::Default,
            };
            let raw_cb = raw.clone();
            let chn = ch as usize;
            match device.build_input_stream(
                &scfg,
                move |data: &[f32], _: &_| push_mono(&raw_cb, data, chn),
                err_fn,
                None,
            ) {
                Ok(s) => {
                    eprintln!("[mic] opened stream: {ch}ch {sr} Hz f32");
                    chosen = Some((s, sr));
                    break 'outer;
                }
                Err(e) => eprintln!("[mic] {ch}ch {sr}Hz f32 -> {e}"),
            }
        }
    }
    let (stream, dev_sr) = chosen.ok_or_else(|| {
        anyhow::anyhow!(
            "could not open any input config — grant microphone permission to the host app \
             ({dev_name:?} was visible but unopenable)"
        )
    })?;
    stream.play()?;

    let start = Instant::now();
    let mut fed_frames = 0usize; // 16 kHz frames already handed to the VAD.
    let mut speech_started = false;
    let mut trailing_silence = 0usize; // consecutive non-speech frames after speech.
    let mut last_vad_ms = 0.0;
    let end_frames = (cfg.end_silence_ms as usize * VAD_SR as usize / 1000) / VAD_FRAME;

    let status = loop {
        std::thread::sleep(Duration::from_millis(30));
        let elapsed = start.elapsed().as_millis() as u64;

        let snapshot = { raw.lock().unwrap().clone() };
        let mono16k = resample_linear(&snapshot, dev_sr, VAD_SR);

        while (fed_frames + 1) * VAD_FRAME <= mono16k.len() {
            let frame = &mono16k[fed_frames * VAD_FRAME..(fed_frames + 1) * VAD_FRAME];
            let t = Instant::now();
            let is_speech = vad.is_speech(frame).unwrap_or(false);
            last_vad_ms = ms(t);
            if is_speech {
                speech_started = true;
                trailing_silence = 0;
            } else if speech_started {
                trailing_silence += 1;
            }
            fed_frames += 1;
        }

        if speech_started && trailing_silence >= end_frames {
            break TurnStatus::EndOfTurn;
        }
        if !speech_started && elapsed >= cfg.initial_silence_ms {
            break TurnStatus::NoSpeech;
        }
        if elapsed >= cfg.max_answer_ms {
            break TurnStatus::MaxAnswer;
        }
    };

    drop(stream); // stop capture
    let final16k = resample_linear(&raw.lock().unwrap(), dev_sr, VAD_SR);
    let audio_secs = final16k.len() as f64 / VAD_SR as f64;
    Ok((
        final16k,
        Turn {
            status,
            last_vad_ms,
            audio_secs,
        },
    ))
}

/// Downmix interleaved frames to mono and append to the shared buffer.
fn push_mono(buf: &Arc<Mutex<Vec<f32>>>, data: &[f32], channels: usize) {
    let mut b = buf.lock().unwrap();
    if channels <= 1 {
        b.extend_from_slice(data);
    } else {
        for frame in data.chunks(channels) {
            b.push(frame.iter().sum::<f32>() / channels as f32);
        }
    }
}

/// Rate-agnostic linear resampler (prototype-grade; good enough for STT).
fn resample_linear(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || input.is_empty() {
        return input.to_vec();
    }
    let ratio = to as f64 / from as f64;
    let out_len = (input.len() as f64 * ratio) as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src = i as f64 / ratio;
        let idx = src.floor() as usize;
        let frac = (src - idx as f64) as f32;
        let a = input.get(idx).copied().unwrap_or(0.0);
        let b = input.get(idx + 1).copied().unwrap_or(a);
        out.push(a + (b - a) * frac);
    }
    out
}

/// Play mono f32 samples at `sr` Hz and block until playback finishes.
pub fn play_blocking(audio: &[f32], sr: u32) -> Result<()> {
    let sink = rodio::DeviceSinkBuilder::open_default_sink()?;
    let buf = rodio::buffer::SamplesBuffer::new(
        NonZero::new(1u16).unwrap(),
        NonZero::new(sr).unwrap(),
        audio.to_vec(),
    );
    sink.mixer().add(buf);
    let dur = Duration::from_secs_f64(audio.len() as f64 / sr as f64 + 0.2);
    std::thread::sleep(dur);
    Ok(())
}
