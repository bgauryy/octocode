---
name: octocode-stats
description: Render an Octocode MCP usage dashboard from `${OCTOCODE_HOME}/stats.json` or `~/.octocode/stats.json`. Use when the user asks to show Octocode stats, usage, tokens/chars saved, cache hits, errors, rate limits, or visualize `stats.json`.
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

- Token counts are estimates: `estimatedTokensSaved = savedChars / 4`. Always describe them as approximate.
- `charsSavedByTool` is cumulative response-savings data, not a recent activity log and not a complete per-tool call ledger.
- Tool response savings may exclude zero-savings tools from visual breakdowns, while total measured calls can still include those calls.
- `rawChars`, `responseChars`, and `savedChars` are measurements from the response pipeline, not a perfect partition where `raw = response + saved`.
- `responseChars` is the final tool response after output pagination; `rawChars` counts upstream API/command/file payloads that were actually fetched or read.
- For paginated upstream APIs, only pages that were actually requested are counted. Later pages appear in stats when those tool calls run.
- Avoid invented comparisons like pages, words, books, or money unless the script explicitly computes them.

## References

- Schema: [packages/octocode-shared/src/session/schemas.ts](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-shared/src/session/schemas.ts)
- Stats path: [packages/octocode-shared/src/paths.ts](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-shared/src/paths.ts)
- Session docs: [packages/octocode-shared/docs/SESSION_PERSISTENCE.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-shared/docs/SESSION_PERSISTENCE.md)
- Config: [docs/CONFIGURATION_REFERENCE.md](https://github.com/bgauryy/octocode-mcp/blob/main/docs/CONFIGURATION_REFERENCE.md)
