# CLI MCP support matrix — Claude Code · Gemini CLI · opencode

Research asset for ticket 0007. Verified against official docs + issue trackers, mid-2026. This backs the contract (0001) and reshapes the code-mode ticket (0006).

## Capability matrix

| Capability | Claude Code | opencode | Gemini CLI |
|---|---|---|---|
| Register local stdio server | ✅ `claude mcp add` / `.mcp.json` | ✅ `mcp.<name>.type:"local"` | ✅ `mcpServers` block |
| 60–90 s blocking `ask` survives | ✅ **natively** (stdio exempt from idle timeout; per-tool default ~28 h) | ⚠️ only with progress heartbeats, **opencode ≥ v1.17.8** | ❌ **~60 s hard wall**; active Jan-2026 bug ignores configured `timeout` |
| Progress notifications extend/keep-alive | N/A on stdio (not needed); progress *rendered* in tool view | ✅ progress **extends timeout** (≥1.17.8) | ❌ not surfaced, does **not** extend timeout |
| Cancel in-flight call | ✅ ESC cancels call w/o killing server; `cancelled` not doc'd verbatim | ⚠️ abort signals reach tools (≥1.17.0); `cancelled` not 100% guaranteed | ❌ broken for long calls (open bug #6116) |
| **Code execution with MCP ("code mode")** | ❌ **not a CLI feature** (uses MCP *Tool Search* for context reduction) | ❌ none native | ❌ none native |
| Skill/command host for wayfinder flow | ✅ plugin bundles skill + `.mcp.json` + binary | ✅ native `SKILL.md` **and reads `.claude/skills/`** + commands/agents | ✅ extensions: TOML commands + `GEMINI.md` + `mcpServers` |

## Headline finding: code mode is unavailable in *every* target CLI

None of the three CLIs support model-written code calling MCP tools in a sandbox. "Code execution with MCP" is an **Anthropic API / Agent SDK** pattern, not exposed in any CLI. Claude Code's context-reduction mechanism is **MCP Tool Search** (deferred tool loading), not code execution. → The Code-Mode premise (spine decision #2) does not hold for CLI hosts; every host does **native per-tool calls**. See "Implications" and ticket 0006.

The good news: our tool surface is tiny (3 tools). Claude Code's **`alwaysLoad: true`** exempts them from tool-search deferral so `begin`/`ask`/`end` are always visible without a search step — the context cost we worried about is small regardless.

## Registration config (per CLI)

**Claude Code** — best host. Ship as a **plugin** (auto-starts the server when enabled, no manual `mcp add`):
```
grill-plugin/
  .claude-plugin/plugin.json
  skills/grill-wayfinder/SKILL.md
  bin/grill-mcp                 # per-platform binary
  .mcp.json
```
```json
// .mcp.json
{ "mcpServers": { "grill": {
  "command": "${CLAUDE_PLUGIN_ROOT}/bin/grill-mcp",
  "args": [], "alwaysLoad": true, "timeout": 180000 } } }
```
- stdio servers are **exempt from the 5-min idle timeout**; default per-tool cap ~28 h. A 90 s `ask` needs no special config (setting an explicit `timeout` is hygiene against a wedged listen).
- Tool names namespaced: `mcp__plugin_grill-plugin_grill__ask`.
- Binary caveats: per-platform blobs + exec bit; don't store runtime state in `${CLAUDE_PLUGIN_ROOT}` (use `${CLAUDE_PLUGIN_DATA}`); or use a wrapper that downloads the binary on first run.

**opencode** — `opencode.json` (project) or `~/.config/opencode/opencode.json` (global):
```json
{ "mcp": { "voice": { "type": "local",
  "command": ["/abs/path/grill-mcp", "--stdio"], "enabled": true } } }
```
- `mcp.<server>.timeout` is **discovery only** (5 s default), NOT execution. Execution timeout ~60 s, no knob to raise — **must emit progress** to survive, and requires **≥ v1.17.8**.
- Reads `.claude/skills/` → the same skill artifact can serve Claude Code AND opencode.

**Gemini CLI** — `.gemini/settings.json` (project) or `~/.gemini/settings.json` (user), or `gemini mcp add`:
```json
{ "mcpServers": { "voice": {
  "command": "/abs/path/grill-mcp", "args": ["--stdio"],
  "timeout": 600000, "trust": false } } }
```
- `timeout` field exists (10-min default) **but** active bug (#17787, Jan 2026; #7324) enforces a hardcoded ~60 s; progress doesn't help; cancellation broken (#6116). Treat Gemini as a **degraded target**.
- Host a wayfinder flow via an **extension** (bundles TOML slash commands + `GEMINI.md` + `mcpServers`).

## Implications for the contract (0001) and spine

1. **Code mode (spine #2) is not realizable in any CLI.** Reframe: context minimization comes from a **tiny tool surface + Claude Code's `alwaysLoad`/Tool Search**, not code execution. Every host uses native per-tool calls — so the "native fallback" is in fact the *only* path. Ticket 0006 must be rewritten accordingly.
2. **Blocking-`ask` viability is per-CLI**, confirming the contract's degradation path is load-bearing:
   - Claude Code: works natively, no cap needed.
   - opencode: works with heartbeats, pin ≥ v1.17.8, beat every ~5–15 s.
   - Gemini: **needs a sub-60 s max-answer cap**; heartbeats/cancellation unavailable → degraded UX.
3. **Cancellation must be defensive everywhere.** The binary must stop listening on `notifications/cancelled` **and** on stdin-close / broken-pipe — never assume a clean `cancelled` arrives.
4. **Packaging:** one skill artifact in `.claude/skills/` covers **Claude Code + opencode**; Gemini needs a TOML+`GEMINI.md` translation. Claude Code plugin (skill + `.mcp.json` + bundled binary) is the low-friction one-install path.
5. **Priority ordering:** Claude Code (first-class) → opencode (good, version-gated) → Gemini (degraded, sub-60 s only).

## Sources
- Claude Code: https://code.claude.com/docs/en/mcp · /skills · /plugins-reference · /changelog · code-execution-with-MCP (API/SDK, not CLI) https://www.anthropic.com/engineering/code-execution-with-mcp
- opencode: https://opencode.ai/docs/mcp-servers/ · /config/ · /skills/ · changelog (v1.17.0, v1.17.8) · issues #23096 #28186 #24965
- Gemini CLI: https://github.com/google-gemini/gemini-cli/blob/main/docs/tools/mcp-server.md · geminicli.com/docs · issues #7324 #17787 #6116 #3052
