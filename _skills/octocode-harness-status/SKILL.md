---
name: octocode-harness-status
description: Show all skills, MCP servers, CLIs, and tokens installed on this machine per vendor in an interactive HTML dashboard. Opens in the default browser. Allows removing MCPs from config files and deleting skill folders directly from the UI. Use when the user asks to "show my harness", "check installed skills", "show my MCPs", "check my AI setup", "show all agents", "harness status", "what CLIs are installed", "check token limits", "review my agent context", or wants an inventory of their full AI tooling.
---

# Octocode Harness Status

Audit every agent vendor on this machine, measure skill context budget, and expose one-click removal — all in a local interactive browser dashboard.

## When to Activate

- "Show my harness", "harness status", "show my AI setup"
- "What skills/MCPs are installed?", "check all vendors"
- "What CLIs are installed and authenticated?"
- "How much context do my skills use?"
- "Remove a skill or MCP", "clean up my agent config"

## Workflow

**One command. That's it.**

```bash
node skills/octocode-harness-status/scripts/build_harness.mjs
```

The script:
1. Reads MCP configs for all supported vendors (Cursor, Claude Code, Claude Desktop, Windsurf, Antigravity, Zed, Codex, Gemini CLI, Kiro, Goose, Trae, VS Code Cline/Roo/Continue, Opencode)
2. Scans all skill directories per vendor
3. Checks installed CLIs and auth status (octocode, gh, claude, cursor, gemini, codex, goose, opencode)
4. Queries GitHub token rate limits
5. Measures SKILL.md context load per vendor and rates it vs typical agent context windows
6. Starts a local HTTP server, opens the dashboard in your default browser
7. Handles live remove/delete actions from the dashboard UI

### Flags

| Flag | Description | Default |
|------|-------------|---------|
| `--port <n>` | HTTP server port | auto (random free port) |
| `--no-open` | Generate but don't open browser | opens |
| `--timeout <s>` | Server auto-shutdown after N seconds | 300 |
| `--help` | Print usage | — |

### Dashboard sections

| Section | What it shows |
|---------|---------------|
| **Summary bar** | Vendor count, total MCPs, total skills, auth status |
| **Per-vendor cards** | MCP list + Skills list, each with a ✕ remove button |
| **CLI inventory** | Version, auth state, rate limits for each CLI |
| **Context budget** | Total SKILL.md bytes per vendor, estimated tokens, % of agent context window, colour-coded health |

### Context rating scale

| Colour | Budget used | Meaning |
|--------|-------------|---------|
| 🟢 Green | < 5 % | Lean — almost no context overhead |
| 🟡 Yellow | 5–15 % | Moderate — normal for active setups |
| 🟠 Orange | 15–30 % | Heavy — consider trimming inactive skills |
| 🔴 Red | > 30 % | Critical — may crowd out useful context |

Context window baselines: Claude 200 k tokens · GPT-4o 128 k tokens · Gemini 1 M tokens · Codex 32 k tokens.

### Remove / edit actions (from the browser UI)

- **Remove MCP** — deletes the server entry from the vendor's JSON config file (preserves other entries)
- **Edit MCP** — edits command/args/type/env in place; **all other fields (`url`, `headers`, `disabled`, `autoApprove`, `cwd`, …) are preserved**, not dropped
- **Remove Skill** — deletes the skill folder (`rm -rf`) from the vendor's skills directory
- **Delete script** — deletes a single script file under the skill

Both actions prompt for confirmation in the dashboard before writing to disk.

**Read-only configs:** TOML (Codex) and YAML (Goose) configs are **parsed for display but never written** — their MCP rows show a 🔒 and have no edit/remove buttons, so the dashboard can't corrupt non-JSON files.

**Safety:** the local server only accepts same-origin requests (no website you visit can drive it), rejects MCP/skill names containing path separators or `..`, and refuses to delete anything outside your home directory or any directory.

### Keyboard & navigation

- `/` — focus the filter box
- `Esc` — close any open modal
- **Expand all / Collapse all** — toggle every vendor card at once

### Server lifecycle

The script prints the dashboard URL and keeps the server alive for `--timeout` seconds (default 5 min). Hit `Ctrl-C` or wait for auto-shutdown.

## Error handling

| Situation | Behaviour |
|-----------|-----------|
| Config file missing | Vendor shown as "not configured" — no error |
| Skills directory missing | Skills count 0 — no error |
| CLI not installed | CLI row shown as "not installed" |
| GitHub rate limit API unreachable | Rate limit shown as "N/A" |
| Port in use | Auto-increments to next free port |
| TOML (Codex) / YAML (Goose) config | Parsed read-only for display; MCP rows are locked (no edit/remove) |
