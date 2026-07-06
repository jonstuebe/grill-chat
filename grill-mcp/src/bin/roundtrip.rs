//! roundtrip — the proven Stage A standalone harness (ticket 0002).
//!
//! Speaks a hardcoded question, captures the mic, endpoints on VAD silence,
//! transcribes, and prints the latency gate. This is the artifact that proved
//! the transport round trip live (142 ms end-of-speech -> transcript, 1 ms turn
//! detection). Kept as a mic bench separate from the MCP server (`main.rs`),
//! which shares the same [`grill_mcp::engine`] pipeline.
//!
//! Env toggles:
//!   GRILL_AUDITION=1  speak the question in each candidate voice, then exit
//!   GRILL_VOICE=<name>  pick a voice (default af_heart)
//!   GRILL_SKIP_TTS=1  skip the speak leg to iterate on the mic path

use grill_mcp::engine::{
    candidate_voices, default_voice, play_blocking, ListenCfg, TurnStatus, VoiceEngine,
};

const QUESTION: &str = "When you picture this finished, what does success actually look like?";
const TTS_SR: u32 = 24_000;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let (mut engine, _timings) = VoiceEngine::load().await?;

    // GRILL_AUDITION=1 -> speak the question in each candidate voice and exit,
    // so we can pick one by ear (playback works even where the mic is blocked).
    if std::env::var("GRILL_AUDITION").is_ok() {
        for name in candidate_voices() {
            println!("\n> {name}");
            engine.set_voice(name.to_string());
            let (audio, _) = engine.synth(&format!("This is {name}. {QUESTION}"))?;
            play_blocking(&audio, TTS_SR)?;
        }
        return Ok(());
    }

    // ---------- SPEAK ---------- (GRILL_SKIP_TTS=1 to iterate on the mic path)
    if std::env::var("GRILL_SKIP_TTS").is_err() {
        let voice = default_voice();
        engine.set_voice(voice.clone());
        println!("\n> speaking (voice={voice}): {QUESTION:?}");
        let (audio, synth_ms) = engine.synth(QUESTION)?;
        // Non-streaming synth: "text -> first audio" == full synth time here.
        println!("[GATE] TTS text->first audio  {synth_ms:>7.0} ms  (target <= 400)");
        play_blocking(&audio, TTS_SR)?;
        println!(
            "[tts] spoke {:.1}s of audio in {:.0} ms wall",
            audio.len() as f64 / TTS_SR as f64,
            synth_ms
        );
    }

    // ---------- LISTEN ----------
    println!("\n🎙️  listening… (speak, then pause)");
    let cfg = ListenCfg::default();
    let (samples16k, turn) = engine.listen(cfg)?;
    match turn.status {
        TurnStatus::NoSpeech => {
            println!("[listen] no speech detected within {} ms", cfg.initial_silence_ms);
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
    let (transcript, stt_ms) = engine.transcribe(&samples16k)?;
    let audio_s = turn.audio_secs;
    println!(
        "[GATE] end-of-speech->transcript {stt_ms:>6.0} ms  (target <= 1000; {:.1}x realtime, {audio_s:.1}s audio)",
        audio_s / (stt_ms / 1000.0),
    );

    println!("\n===> TRANSCRIPT: {transcript:?}");
    Ok(())
}
