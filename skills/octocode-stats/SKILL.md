---
name: octocode-stats
description: Use when the user asks to show Octocode stats, usage, a dashboard, tokens/chars saved, cache hits, errors, or rate limits — renders an Octocode MCP usage dashboard from `${OCTOCODE_HOME}/stats.json` or `~/.octocode/stats.json`.
---

# Octocode Stats Dashboard

Generate a local HTML dashboard from Octocode usage stats. The build script owns all parsing, calculations, rendering, and browser-opening behavior; do not reimplement those calculations in chat or another script.

## When to Activate

Use this skill when the user asks to:

- Show Octocode stats, usage, dashboard, or report
- Check tokens/chars saved
- Inspect tool response savings
- Inspect GitHub cache hits or avoided rate limits
- Compare raw GitHub/package payload size with the final tool response sent back
- Review errors, rate limits, or package registry failures
- Render `stats.json` or data under `~/.octocode/`

Do not use it for session ID lookup, credentials, or unrelated configuration files.

## Workflow

1. Resolve the stats file:
   - `${OCTOCODE_HOME}/stats.json` when `OCTOCODE_HOME` is set
   - `~/.octocode/stats.json` otherwise
   - A user-provided path via `--stats <path>` when explicitly requested

2. If the stats file is missing, say:
   `No stats yet — run any Octocode MCP tool first, then re-run this skill.`

3. Run the dashboard builder from the repo root:

```bash
node skills/octocode-stats/scripts/build_dashboard.mjs
```

Common flags:

- `--stats <path>`: read a non-default stats file
- `--output <path>`: write the dashboard somewhere else
- `--no-open`: generate without opening the browser
- `--allow-empty`: render an empty-state dashboard even when stats are missing
- `--help`: print script usage

4. Report only the useful result:
   - Dashboard path
   - Stats source path
   - Total tool calls
   - Estimated tokens saved
   - GitHub cache hits
   - Errors

If the browser did not open, include a manual `open <dashboard-path>` command on macOS.

## Measurement Notes

Read `references/measurement-notes.md` before interpreting or describing the numbers — token estimates, cumulative vs per-call savings, and what `rawChars`/`responseChars`/`savedChars` do and don't partition.

## Source references

- Schema: [packages/octocode-tools-core/src/shared/session/schemas.ts](https://github.com/bgauryy/octocode/blob/main/packages/octocode-tools-core/src/shared/session/schemas.ts)
- Stats path: [packages/octocode-tools-core/src/shared/paths.ts](https://github.com/bgauryy/octocode/blob/main/packages/octocode-tools-core/src/shared/paths.ts)
- Session docs: [docs/mcp/SESSION.md](https://github.com/bgauryy/octocode/blob/main/docs/mcp/SESSION.md)
- Config: [docs/mcp/CONFIGURATION.md](https://github.com/bgauryy/octocode/blob/main/docs/mcp/CONFIGURATION.md)
