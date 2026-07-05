---
id: "0005"
title: Design the voice-grill skill loop and summary
type: wayfinder:grilling
status: closed
assignee: jonstuebe
blocked_by: ["0001"]
---

## Question

How does the `voice-grill` skill drive the loop, and what does it hand back? Design:

- The **loop prompt**: how the skill calls `ask`, reasons about each transcript in main context, and generates the next question (clear → next distinct question; ambiguous → follow-up; contradicts earlier → surface it).
- **Termination**: turn/time budget default before it wraps up on its own; recognizing user stop-intent as a control signal; user-initiated stop.
- **Handoff**: the final structured summary format (resolved decisions, open questions, action items) written into the session so the user resumes coding with ambiguity captured.
- **Relationship to existing grill skills** — should this feel like a sibling of `/grilling` / `wayfinder`, reusing their prompting patterns?

Resolves into a skill spec (the skill half of the destination). Depends on the contract (0001) since the skill calls its tools.

## Resolution

**`voice-grill` is a voice *mode of wayfinder*, not a standalone skill and not a wrapper around `grill-me`/`grilled-docs`.** Wayfinder runs exactly as it does today; the only change is that its **grilling dialogue turns** are conducted through the voice MCP tools instead of typed text. This keeps a single wayfinding brain (no drift) and matches the original doc's principle: swap the I/O modality for *the conversational portion only*.

### What speaks vs what stays textual

- **Spoken (via `ask()`):** the grilling dialogue — destination-naming, breadth-first frontier-mapping, and resolving grilling-type tickets.
- **Silent / textual:** all tracker mechanics — creating the map & tickets, wiring blocking, writing resolutions, updating Decisions-so-far, and git commits. Nobody wants ticket frontmatter or a commit message read aloud.

### Spoken-question discipline (the loop)

Wayfinder questions always carry enumerated options **plus a recommendation**. Over voice that renders as **the recommendation stated as a proposal**, not a read-aloud menu:

- Each turn speaks one or two conversational sentences — *"I'd suggest [recommended answer, plainly] because [one reason]. Does that work, or do you see it differently?"* — one idea per turn, no markdown, no A/B/C enumeration aloud.
- **The terminal simultaneously shows the rich form** (full options + rationale), so the modalities complement: listen-and-respond by default, glance-at-terminal for the menu when wanted.
- Reasoning still happens in the main text session (per the spine); only the spoken line is disciplined.

### Termination & handoff

- **Phase-bounded, no artificial turn/time cap.** A session runs until the wayfinder phase naturally completes (charting: destination named + frontier mapped; working: one ticket resolved). A hard cap is rejected — truncating a grill mid-thought is the exact failure to avoid.
- **One natural checkpoint** at the big charting boundary (after the destination is named, before frontier-mapping): "keep going by voice, pause, or switch to text?"
- **Graceful stop, always-flush.** The skill infers stop-intent from a transcript (contract's smart/dumb split), speaks a brief confirm, then ends the voice session and **writes whatever's resolved so far into the map/tickets as partial updates** — a voice session never evaporates unrecorded. Then hands back to text.

### Open details (not blocking; belong downstream)

- **Invocation**: user runs wayfinder in a voice mode (flag / companion trigger); the precise trigger + auto-spawn of the binary overlap with CLI registration (0007) and the auto-spawn Fog item.
- **Low-confidence policy**: when `answer.confidence` is low, re-ask vs proceed-and-flag-in-summary — a skill policy to settle during implementation.

