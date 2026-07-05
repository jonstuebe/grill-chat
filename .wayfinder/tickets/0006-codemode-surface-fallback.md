---
id: "0006"
title: Package as a Claude Code plugin (skill + MCP binary)
type: wayfinder:prototype
status: open
assignee:
blocked_by: ["0002"]
---

## Question

> **Reframed by 0007.** Original scope was "code-mode surface + native-tool fallback across CLIs." Research (0007) found **code mode is unavailable in every CLI** and narrowed scope to **Claude Code only**, so there is no code-mode surface and no cross-CLI fallback to build. What remains is the packaging that makes install one-step.

How is the whole thing delivered as a single low-friction Claude Code install? Design and prove a **plugin** that bundles both the skill and the MCP server:

```
grill-plugin/
  .claude-plugin/plugin.json
  skills/grill-wayfinder/SKILL.md
  bin/grill-mcp                 # the Rust binary
  .mcp.json                     # command: ${CLAUDE_PLUGIN_ROOT}/bin/grill-mcp, alwaysLoad: true
```

Resolve:
- **`.mcp.json`** pointing at `${CLAUDE_PLUGIN_ROOT}/bin/grill-mcp`, `alwaysLoad: true` (3-tool surface always visible, no ToolSearch step), explicit `timeout` as hygiene.
- **Binary distribution**: per-platform blobs + exec bit vendored in the plugin, OR a thin wrapper that downloads/builds on first run. Keep runtime state in `${CLAUDE_PLUGIN_DATA}` (plugin root changes on update).
- **Model weights**: bundled vs first-run download (ties to 0003's footprint decision).
- **Marketplace** distribution (`marketplace.json`), `defaultEnabled: false`.

Resolves when a `/plugin install` brings up the skill + auto-started MCP server and a grilling turn works end to end on Claude Code. Depends on the transport prototype (0002).
