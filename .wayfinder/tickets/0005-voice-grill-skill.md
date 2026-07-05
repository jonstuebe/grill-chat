---
id: "0005"
title: Design the voice-grill skill loop and summary
type: wayfinder:grilling
status: open
assignee:
blocked_by: ["0001"]
---

## Question

How does the `voice-grill` skill drive the loop, and what does it hand back? Design:

- The **loop prompt**: how the skill calls `ask`, reasons about each transcript in main context, and generates the next question (clear → next distinct question; ambiguous → follow-up; contradicts earlier → surface it).
- **Termination**: turn/time budget default before it wraps up on its own; recognizing user stop-intent as a control signal; user-initiated stop.
- **Handoff**: the final structured summary format (resolved decisions, open questions, action items) written into the session so the user resumes coding with ambiguity captured.
- **Relationship to existing grill skills** — should this feel like a sibling of `/grilling` / `wayfinder`, reusing their prompting patterns?

Resolves into a skill spec (the skill half of the destination). Depends on the contract (0001) since the skill calls its tools.
