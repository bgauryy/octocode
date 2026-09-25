# orangu with Octocode

Load when running OBSERVE or reading orangu JSON fields.

[orangu](https://www.npmjs.com/package/orangu) (MIT, zero runtime dependencies, Node ≥20) parses the Claude Code `.jsonl` transcripts already on disk. It needs no network, SDK, or hook. Pin a version (`npx -y orangu@0.7.2`) so repeated runs compare like with like.

## Commands that matter here

| Command | Use for Octocode | Key fields |
|---|---|---|
| `orangu harness --cwd . --json --quiet` | Is the Octocode MCP server configured and used, and what does the preamble weigh | `inventory.mcpServers`, `inventory.claudeMd[].approxTokens`, `crosswalk.mcpServers[].{toolCalls,distinctTools,status}`, `crosswalk.skills[]` (octocode skill usage) |
| `orangu repo . --json --quiet --limit 60` | Per-tool calls/errors across this repo's sessions | `byTool[]` where `key` starts `mcp__octocode` → `extra.{calls,errors}`; `crossFindings[]` (`oversized-tool-results`, `tool-errors`, `reread-files`, `preamble-weight`); `topReReadFiles` |
| `orangu analyze <session\|latest\|current> --json --slim` | One session in depth (for example the session that ran the probe) | `summary.toolCalls/toolErrors`, `tokens` |
| `orangu evidence <session>` | Bounded findings plus known fixes for one session | Use `--estimate` first. `--scope` works only on aggregate JSON input |
| `orangu estimate repo` | Size check before an agent reads orangu output | bytes, ≈tokens |
| `orangu report latest` / `orangu serve` | Human HTML views | Not needed by agents |

## Limits to plan around

- `recurringErrors[].signature` is empty under default redaction. Use `scripts/mine-transcripts.mjs` for error text.
- `byTool[].tokens` is 0: orangu does not attribute tokens to individual tools. Use output chars from `mine-transcripts` (history) and `probe-surfaces` (live).
- Errors include host-side failures (auto-mode classifier outage, user rejections). Triage them before blaming a tool.
- History mixes builds. Use `--since <date>` in `mine-transcripts` to isolate the current binary, and compare first/last dates of each signature.
- Subagent transcripts live under the same project directory. The miner walks them, and orangu counts them.

## Shell gotcha (zsh)

Do not store the command in a variable (`O="node …/orangu.js"; $O repo`). zsh does not word-split it, and the call fails with `no such file or directory`. Call `npx -y orangu@0.7.2 …` directly or define a function.
