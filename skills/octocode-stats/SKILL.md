---
name: octocode-stats
description: "Use when the user asks for Octocode usage stats or a dashboard: tokens/chars saved, cache hits, tool-call counts, response-size savings, errors, rate limits, or a stats.json report from OCTOCODE_HOME or ~/.octocode."
---

# Octocode Stats Dashboard

Generate a local HTML dashboard from Octocode usage stats. The build script owns parsing, calculations, rendering, and optional browser opening. MUST NOT reimplement the math in chat — report only the numbers the script emits.

Flow: `RESOLVE STATS -> RUN BUILDER -> REPORT PATH + KEY NUMBERS`.

## When to use

- Octocode stats, usage dashboard, saved tokens/chars, cache hits, rate-limit avoidance, errors, or registry failures.
- Rendering an explicit stats file, `${OCTOCODE_HOME}/stats.json`, or `~/.octocode/stats.json`.

Skip credentials, MCP install, and unrelated configuration questions.

## Workflow

1. Resolve stats path in order: user `--stats`, then `${OCTOCODE_HOME}/stats.json`, then `~/.octocode/stats.json`.
2. IF no stats file resolves THEN reply `No stats yet - run any Octocode MCP tool first, then re-run this skill.` and stop.
3. Run `node skills/octocode-stats/scripts/build_dashboard.mjs`. Add `--open` only when the user asked to view the dashboard immediately.
4. Report dashboard path, stats source, total calls, estimated savings, cache hits, and errors.

Common flags: `--stats <path>`, `--output <path>`, `--open`, `--no-open`, `--allow-empty`, `--help`.

## Reference Map

- `references/measurement-notes.md` — before interpreting token estimates, raw/response/saved chars, or cumulative metrics.
- `references/octocode.md` — when choosing transport, auth, install, or CLI/MCP fallback behavior.

## Scripts

- `scripts/build_dashboard.mjs` — deterministic dashboard builder for stats parsing, calculations, HTML rendering, and optional browser open.

## Installation

```bash
npx octocode skill --name octocode-stats
```
