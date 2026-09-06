# @octocodeai/pi-extension docs

Docs in this directory belong to the Pi harness extension.
Keep harness runtime, bundled tools, Awareness wiring, prompt/override behavior,
and Pi UI notes here. Exact tool field schemas remain generated/runtime-owned:
use `node $OCTOCODE_CLI tools <name> --scheme`.

## Index

### Core references

| Document | Owns |
|---|---|
| [ARCHITECTURE.md](https://github.com/bgauryy/octocode/blob/main/packages/octocode-pi-extension/ARCHITECTURE.md) | Pi composition root, adapter ownership, dependency rules, and convergence limits. |
| [TOOLS.md](https://github.com/bgauryy/octocode/blob/main/packages/octocode-pi-extension/docs/TOOLS.md) | Complete tool inventory, routing rules, CLI schema lookup, and Awareness-as-CLI guidance. |
| [OVERRIDES.md](https://github.com/bgauryy/octocode/blob/main/packages/octocode-pi-extension/docs/OVERRIDES.md) | Branded-launcher native-tool suppression, direct-extension backstop, and replacement routes. |
| [WHY_OCTOCODE.md](https://github.com/bgauryy/octocode/blob/main/packages/octocode-pi-extension/docs/WHY_OCTOCODE.md) | Product positioning, capability profiles, and comparison with vanilla Pi. |

### Media and FFmpeg

| Document | Owns |
|---|---|
| [FFMPEG.md](https://github.com/bgauryy/octocode/blob/main/packages/octocode-pi-extension/docs/FFMPEG.md) | Complete ffmpeg guide: discovery, exact argvs, `runFfmpeg` reference, routing, anti-patterns, 16 cookbook recipes, hardware encoding, screen capture, bundling, and capability matrix. |
| [MEDIA_TOOL.md](https://github.com/bgauryy/octocode/blob/main/packages/octocode-pi-extension/docs/MEDIA_TOOL.md) | RFC and routing contract for `readMedia` vs `media` (two-tool effect boundary). |

### Agent coordination and subagents

| Document | Owns |
|---|---|
| [AWARENESS_AGENT_FLOW.md](https://github.com/bgauryy/octocode/blob/main/packages/octocode-pi-extension/docs/AWARENESS_AGENT_FLOW.md) | Agent lifecycle for using Awareness inside Pi sessions. |
| [AGENT_ORCHESTRATOR.md](https://github.com/bgauryy/octocode/blob/main/packages/octocode-pi-extension/docs/AGENT_ORCHESTRATOR.md) | Pi SDK subagent orchestration contract, rollback notes, and UX policy. |
| [SUBAGENTS.md](https://github.com/bgauryy/octocode/blob/main/packages/octocode-pi-extension/docs/SUBAGENTS.md) | Spawn profiles, live control, durable peer communication, and isolation. |
| [REFLECT.md](https://github.com/bgauryy/octocode/blob/main/packages/octocode-pi-extension/docs/REFLECT.md) | Reflection and memory workflow as exposed through the harness. |

### Runtime and TUI

| Document | Owns |
|---|---|
| [UI.md](https://github.com/bgauryy/octocode/blob/main/packages/octocode-pi-extension/docs/UI.md) | TUI design contract, widget inventory, responsive layout, core flows, and troubleshooting. |
| [SETTINGS.md](https://github.com/bgauryy/octocode/blob/main/packages/octocode-pi-extension/docs/SETTINGS.md) | Complete `/settings` control-center reference: commands, MCP, discovery, tools, skills, persistence, security, refresh behavior, and limitations. |
| [RUNTIME_STATE.md](https://github.com/bgauryy/octocode/blob/main/packages/octocode-pi-extension/docs/RUNTIME_STATE.md) | Session initialization, Zustand state ownership, MCP readiness, and disposal. |
| [SESSION_ARTIFACTS.md](https://github.com/bgauryy/octocode/blob/main/packages/octocode-pi-extension/docs/SESSION_ARTIFACTS.md) | Where session files live (plans, screenshots, logs, compaction snapshots), manifest, and cleanup. |
| [CRON.md](https://github.com/bgauryy/octocode/blob/main/packages/octocode-pi-extension/docs/CRON.md) | Session job safety model, default jobs, and cron-style maintenance commands. |

### Audit and decisions

| Document | Owns |
|---|---|
| [AGENT_TOOL_AUDIT.md](https://github.com/bgauryy/octocode/blob/main/packages/octocode-pi-extension/docs/AGENT_TOOL_AUDIT.md) | Dated decision snapshot for palette ratings, Awareness signal value, and contract-size evidence. Current registries remain source-owned. |

---

The repository root documentation covers harness-wide capability discovery
(MCP catalog, `skill` tool, `.octocode/discovery.json`, and context composition):
[`docs/OCTOCODE_TOOLS.md`](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_TOOLS.md).

Do not add package command catalogs here. Package scripts are manifest-owned;
user-facing launcher commands belong in
[the CLI guide](https://github.com/bgauryy/octocode/blob/main/packages/octocode/docs/OCTOCODE_CLI.md) or launcher help.
