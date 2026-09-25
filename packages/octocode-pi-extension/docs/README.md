# @octocodeai/pi-extension docs

## Index

### Core references

| Document | Owns |
|---|---|
| [ARCHITECTURE.md](https://github.com/bgauryy/octocode/blob/main/packages/octocode-pi-extension/ARCHITECTURE.md) | Pi composition root, adapter ownership, dependency rules, and convergence limits. |
| [CAPABILITIES.md](CAPABILITIES.md) | Native and foreign sources, reviewed linked imports, effective skill selection, model/hook adapters, versioned catalogs, and worker grants. |
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
| [AGENT_ORCHESTRATOR.md](https://github.com/bgauryy/octocode/blob/main/packages/octocode-pi-extension/docs/AGENT_ORCHESTRATOR.md) | Pi SDK subagent orchestration contract, rollback notes, and UX policy. |
| [SUBAGENTS.md](https://github.com/bgauryy/octocode/blob/main/packages/octocode-pi-extension/docs/SUBAGENTS.md) | Spawn profiles, live control, durable peer communication, and isolation. |

### Runtime and TUI

| Document | Owns |
|---|---|
| [UI.md](https://github.com/bgauryy/octocode/blob/main/packages/octocode-pi-extension/docs/UI.md) | Current TUI design contract, widget inventory, responsive layout, core flows, and troubleshooting. |
| [STATUS_PROGRESS_UX.md](https://github.com/bgauryy/octocode/blob/main/packages/octocode-pi-extension/docs/STATUS_PROGRESS_UX.md) | Target adaptive UX for footer metadata, progress, plans, tasks, agents, messages, attention, and verification. |
| [SETTINGS.md](SETTINGS.md) | `/config` OS browser control center: commands, MCP, source review, skills, models, hooks, persistence, security, and next-turn refresh. |
| [RUNTIME_STATE.md](https://github.com/bgauryy/octocode/blob/main/packages/octocode-pi-extension/docs/RUNTIME_STATE.md) | Session initialization, Zustand state ownership, MCP readiness, and disposal. |
| [SESSION_ARTIFACTS.md](https://github.com/bgauryy/octocode/blob/main/packages/octocode-pi-extension/docs/SESSION_ARTIFACTS.md) | Where session files live (plans, screenshots, logs, compaction snapshots), manifest, and cleanup. |

### Audit and decisions

| Document | Owns |
|---|---|
| [BEST_PRACTICES.adoc](BEST_PRACTICES.adoc) | Researched guidance and a prioritized implementation backlog for tools, subagents, widgets, lifecycle, and Pi compatibility. |

---

The repository root documentation covers harness-wide capability discovery
(MCP catalog, `skill` tool, discovery artifacts, and context composition):
[`docs/OCTOCODE_TOOLS.md`](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_TOOLS.md).

Do not add package command catalogs here. Package scripts are manifest-owned;
user-facing launcher commands belong in
[the CLI guide](https://github.com/bgauryy/octocode/blob/main/packages/octocode/docs/OCTOCODE_CLI.md) or launcher help.

## Communication and session persistence

The bundled communication runtime owns peer presence, messages, documents, and advisory leases. Pi owns plans, context recovery, and durable user approvals. See [communication and local state](COMMUNICATION_AGENT_FLOW.md).
