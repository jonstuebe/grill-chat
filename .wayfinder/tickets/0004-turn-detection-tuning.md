---
id: "0004"
title: Turn detection integration and false-ending tuning
type: wayfinder:prototype
status: open
assignee:
blocked_by: ["0002"]
---

## Question

Does the endpointing feel right — specifically, does it avoid cutting the user off mid-thought (the top edge case)? Integrate Silero VAD (silence gate) + `smart-turn-v3` (semantic "actually finished" probability on the trailing audio window), port smart-turn-v3's log-mel preprocessing to Rust, and tune the VAD hangover + probability threshold on real speech with deliberate mid-sentence pauses ("I think the auth flow should… let me think…").

Resolves into a tuned endpointing config + notes on the false-turn-ending / false-interruption trade-offs (linked asset). Because the interaction is half-duplex, this is a VAD-hangover + one probability check, not full-duplex barge-in machinery.
