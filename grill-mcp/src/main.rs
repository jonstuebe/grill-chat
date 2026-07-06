//! grill-mcp — transport prototype (ticket 0002), Stage A: standalone
//! voice round-trip. Speak a hardcoded question (Kokoro TTS -> rodio),
//! capture the mic (cpal), endpoint on VAD-silence (transcribe-rs Silero
//! VAD), transcribe (Parakeet), print transcript + latency numbers.
//!
//! Semantic end-of-turn (smart-turn-v3) is deliberately deferred to the
//! turn-detection-tuning ticket (0004); trailing-silence VAD is enough to
//! prove the transport round-trip and measure the latency gate.

use std::num::NonZero;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use kokoros::tts::koko::TTSKoko;
use transcribe_rs::onnx::parakeet::{ParakeetModel, ParakeetParams};
use transcribe_rs::onnx::Quantization;
use transcribe_rs::vad::{SileroVad, SmoothedVad, Vad};

const QUESTION: &str = "When you picture this finished, what does success actually look like?";

// Model artifacts (downloaded into ./models on first run).
// TTS uses the `kokoros` crate + fp32 Kokoro-v1.0 (same engine/weights as the
// `yap` project) — noticeably more natural English than the int8 mzdk100 fork.
const KOKORO_MODEL: &str = "models/kokoro-v1.0.onnx";
const KOKORO_VOICES: &str = "models/voices-v1.0.bin";
const TTS_LANG: &str = "en-us";
const TTS_SPEED: f32 = 1.0;
const PARAKEET_DIR: &str = "models/parakeet-tdt-0.6b-v3-int8";
const SILERO_VAD: &str = "models/silero_vad_v4.onnx";

// Endpointing thresholds.
const VAD_SR: u32 = 16_000; // Silero VAD + Parakeet operate at 16 kHz.
const VAD_FRAME: usize = 480; // 30 ms @ 16 kHz (SileroVad::frame_size()).
const END_SILENCE_MS: u64 = 800; // trailing silence that ends a turn.
const INITIAL_SILENCE_MS: u64 = 8_000; // give up if the user never speaks.
const MAX_ANSWER_MS: u64 = 30_000; // hard cap on a single answer.

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    for p in [KOKORO_MODEL, KOKORO_VOICES, PARAKEET_DIR, SILERO_VAD] {
        if !std::path::Path::new(p).exists() {
            anyhow::bail!("missing model artifact: {p} (run the model download first)");
        }
    }

    // --- Load models (lazy in a real server; eager here to isolate timing). ---
    let t = Instant::now();
    let tts = TTSKoko::new(KOKORO_MODEL, KOKORO_VOICES).await;
    println!("[load] Kokoro TTS      {:>7.0} ms", t.elapsed().as_secs_f64() * 1000.0);

    // GRILL_AUDITION=1 -> speak the question in each candidate voice and exit,
    // so we can pick one by ear (playback works even where the mic is blocked).
    if std::env::var("GRILL_AUDITION").is_ok() {
        for name in candidate_voices() {
            println!("\n> {name}");
            let audio = tts.tts_raw_audio(
                &format!("This is {name}. {QUESTION}"),
                TTS_LANG, name, TTS_SPEED, None, None, None, None,
            ).map_err(|e| anyhow::anyhow!("synth failed: {e}"))?;
            play_blocking(&audio, 24_000)?;
        }
        return Ok(());
    }

    let t = Instant::now();
    let mut stt = ParakeetModel::load(&PathBuf::from(PARAKEET_DIR), &Quantization::Int8)?;
    println!("[load] Parakeet STT    {:>7.0} ms", t.elapsed().as_secs_f64() * 1000.0);

    let t = Instant::now();
    let silero = SileroVad::new(&PathBuf::from(SILERO_VAD), 0.3)?;
    let mut vad = SmoothedVad::new(Box::new(silero), 15, 15, 2);
    println!("[load] Silero VAD      {:>7.0} ms", t.elapsed().as_secs_f64() * 1000.0);

    // ---------- SPEAK ---------- (GRILL_SKIP_TTS=1 to iterate on the mic path)
    if std::env::var("GRILL_SKIP_TTS").is_err() {
        let voice = voice_from_env();
        println!("\n> speaking (voice={voice}): {QUESTION:?}");
        let t_tts = Instant::now();
        let audio = tts
            .tts_raw_audio(QUESTION, TTS_LANG, &voice, TTS_SPEED, None, None, None, None)
            .map_err(|e| anyhow::anyhow!("synth failed: {e}"))?;
        // Non-streaming synth: "text -> first audio" == full synth time here.
        println!(
            "[GATE] TTS text->first audio  {:>7.0} ms  (target <= 400)",
            t_tts.elapsed().as_secs_f64() * 1000.0
        );
        play_blocking(&audio, 24_000)?;
        println!("[tts] spoke {:.1}s of audio in {:.0} ms wall",
            audio.len() as f64 / 24_000.0, t_tts.elapsed().as_secs_f64() * 1000.0);
    }

    // ---------- LISTEN ----------
    println!("\n🎙️  listening… (speak, then pause)");
    let (samples16k, turn) = listen(&mut vad)?;
    match turn.status {
        TurnStatus::NoSpeech => {
            println!("[listen] no speech detected within {INITIAL_SILENCE_MS} ms");
            return Ok(());
        }
        TurnStatus::MaxAnswer => println!("[listen] hit max-answer cap"),
        TurnStatus::EndOfTurn => {}
    }
    println!(
        "[GATE] turn detection (last frame) {:>4.0} ms  (target <= 100)",
        turn.last_vad_ms
    );

    // ---------- TRANSCRIBE ----------
    let t_stt = Instant::now();
    let result = stt.transcribe_with(&samples16k, &ParakeetParams::default())?;
    let stt_ms = t_stt.elapsed().as_secs_f64() * 1000.0;
    let audio_s = samples16k.len() as f64 / VAD_SR as f64;
    println!(
        "[GATE] end-of-speech->transcript {:>6.0} ms  (target <= 1000; {:.1}x realtime, {:.1}s audio)",
        stt_ms,
        audio_s / (stt_ms / 1000.0),
        audio_s
    );

    println!("\n===> TRANSCRIPT: {:?}", result.text.trim());
    Ok(())
}

#[derive(Debug)]
enum TurnStatus {
    EndOfTurn,
    NoSpeech,
    MaxAnswer,
}

struct Turn {
    status: TurnStatus,
    last_vad_ms: f64, // cost of the final VAD frame inference.
}

/// Capture the default input device until the turn ends (trailing silence),
/// returning 16 kHz mono f32 samples ready for Parakeet.
fn listen(vad: &mut SmoothedVad) -> anyhow::Result<(Vec<f32>, Turn)> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| anyhow::anyhow!("no input device"))?;
    #[allow(deprecated)]
    let dev_name = device.name().unwrap_or_default();
    println!("[mic] device: {dev_name:?}");

    // Raw mono f32 at the device rate, filled from the audio callback.
    let raw = Arc::new(Mutex::new(Vec::<f32>::new()));
    let err_fn = |e| eprintln!("stream error: {e}");

    // Attempt build_input_stream directly with explicit configs. This *device
    // access* is what triggers the macOS mic-permission prompt (a property
    // query like default_input_config() only errors when unauthorized). We try
    // a few common (rate, channels, format) combos and keep the first that
    // opens — also learning the real device rate/channels for resampling.
    let mut chosen: Option<(cpal::Stream, u32, usize)> = None;
    'outer: for &sr in &[48_000u32, 44_100, 16_000] {
        for &ch in &[1u16, 2] {
            // NOTE: under a host app without mic entitlement (e.g. an embedded
            // session host), these opens fail with misleading "device
            // unavailable" errors and no TCC prompt. Run from a normal terminal
            // that can prompt for microphone access. See ticket 0002 / map Fog.
            let cfg = cpal::StreamConfig {
                channels: ch,
                sample_rate: sr,
                buffer_size: cpal::BufferSize::Default,
            };
            let raw_cb = raw.clone();
            let chn = ch as usize;
            // Mic default format is f32 on macOS; build an f32 input stream.
            match device.build_input_stream(
                &cfg,
                move |data: &[f32], _: &_| push_mono(&raw_cb, data, chn),
                err_fn,
                None,
            ) {
                Ok(s) => {
                    println!("[mic] opened stream: {ch}ch {sr} Hz f32");
                    chosen = Some((s, sr, chn));
                    break 'outer;
                }
                Err(e) => println!("[mic] {ch}ch {sr}Hz f32 -> {e}"),
            }
        }
    }
    let (stream, dev_sr, _channels) = chosen.ok_or_else(|| {
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
    let end_frames = (END_SILENCE_MS as usize * VAD_SR as usize / 1000) / VAD_FRAME;

    let status = loop {
        std::thread::sleep(Duration::from_millis(30));
        let elapsed = start.elapsed().as_millis() as u64;

        let snapshot = { raw.lock().unwrap().clone() };
        let mono16k = resample_linear(&snapshot, dev_sr, VAD_SR);

        while (fed_frames + 1) * VAD_FRAME <= mono16k.len() {
            let frame = &mono16k[fed_frames * VAD_FRAME..(fed_frames + 1) * VAD_FRAME];
            let t = Instant::now();
            let is_speech = vad.is_speech(frame).unwrap_or(false);
            last_vad_ms = t.elapsed().as_secs_f64() * 1000.0;
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
        if !speech_started && elapsed >= INITIAL_SILENCE_MS {
            break TurnStatus::NoSpeech;
        }
        if elapsed >= MAX_ANSWER_MS {
            break TurnStatus::MaxAnswer;
        }
    };

    drop(stream); // stop capture
    let final16k = resample_linear(&raw.lock().unwrap(), dev_sr, VAD_SR);
    Ok((final16k, Turn { status, last_vad_ms }))
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

/// Candidate interview voices to audition (kokoros voice names).
fn candidate_voices() -> Vec<&'static str> {
    vec![
        "af_heart", "af_bella", "af_nicole", "af_aoede", "am_michael", "am_puck",
        "am_fenrir", "bf_emma", "bm_george",
    ]
}

/// Voice selected by GRILL_VOICE, else af_heart (yap's default).
fn voice_from_env() -> String {
    std::env::var("GRILL_VOICE").unwrap_or_else(|_| "af_heart".into())
}

/// Play mono f32 samples at `sr` Hz and block until playback finishes.
fn play_blocking(audio: &[f32], sr: u32) -> anyhow::Result<()> {
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
