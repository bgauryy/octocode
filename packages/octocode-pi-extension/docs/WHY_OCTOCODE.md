# Why Octocode on Pi

Octocode is the opinionated, evidence-first harness layer for Pi. Use it when a task needs more than a minimal chat/edit loop: codebase research, verifiable edits, multi-agent coordination, durable workflow skills, browser/MCP/web surfaces, and explicit safety gates.

## 30-second positioning

| If you want… | Use… | Why |
|---|---|---|
| Minimal Pi with your own extensions | vanilla Pi | Smallest surface and least policy. |
| MCP only | `pi-mcp-adapter` or Octocode Lite profile | Octocode includes MCP, but MCP-only users may prefer a small package. |
| Web fetching only | `pi-web-access` or Octocode Lite profile | Octocode adds web plus local/GitHub/LSP research and browser automation. |
| Simple subagents | `pi-subagents` | Focused delegation package. |

Octocode is not trying to be the smallest Pi package. It is the professional harness profile for teams and power users who want the agent to prove what it read, coordinate shared work, and finish with verification evidence.

## Capability profiles

These profiles describe how the extension should be explained and configured. They are product modes, not separate packages yet.

| Profile | Best for | Surface |
|---|---|---|
| Lite | First run, cautious users, MCP/web tasks | Guarded file/bash, MCPTool, web, askUser, and basic docs. |

## Where Octocode should win

## Where Octocode should not overclaim

- **Simplicity:** vanilla Pi, Aider, Codex, Gemini CLI, Goose, and Crush are easier to explain for one-off use.
- **Visual polish:** UI-first packages may beat Octocode on focused plan review or TUI delight.
- **Single-purpose install size:** specialized packages are easier to audit when a user only wants one feature.

## Product rule

Every new capability should answer one of these questions:

1. Does it make evidence easier to gather or inspect?
2. Does it make edits safer or more reversible?
3. Does it make multi-agent/shared-repo work clearer?
4. Does it make first-run comprehension simpler?

If not, keep it out of the default surface or put it behind a Pro/profile path.
