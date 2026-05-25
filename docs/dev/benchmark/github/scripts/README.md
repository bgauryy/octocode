# Scripts

Zero-dependency. Every measurement is **chars (codepoints) + wall time + count** — same ruler for both agents.

| Script | What it does |
|---|---|
| `init-run.sh <octocode\|gh\|none>` | Create `output/<ts>-<agent>/q{1..31}/` and `log.jsonl`. Exports `$RUN`, `$LOG`. |
| `gh-meas.sh <gh args>` | Runs `gh`, appends `{q, in_chars, out_chars, elapsed_ms}` to `$LOG`, passes stdout through. |
| `mcp-meas.mjs <mcp-server-cmd> [args...]` | **Transparent MCP stdio proxy.** Spawns the Octocode MCP server and logs every `tools/call` pair (request + response, by JSON-RPC `id`) to `$LOG` with the same schema as `gh-meas.sh`. Fully automated — both sides measured equivalently. |
| `octo-meas.sh <tool> <req_file> <res_file> [elapsed_ms]` | Manual fallback when the MCP proxy can't be used. Agent invokes after each call. |
| `chars.mjs` | Codepoint counter. `string` → stdout. |
| `aggregate.mjs <log> <q>` | Sums one question's log rows. Prints `calls in out ms`. |
| `record.sh <q> <model> <answer_file>` | Writes `$RUN/q<q>/output.md` using metrics from `$LOG`. |
| `finalize.mjs <run>` | Writes `<run>/output.md` summary table. |
| `cross-run.mjs <run...>` | Median across ≥2 runs of the same agent. |
| `judge.mjs <run_a> <run_b>` | Auto-score using `EXPECTED_FACTS.md` (verbatim-token match). |
| `verify-facts.mjs` | Re-fetch every file path in `EXPECTED_FACTS.md`; detect drift. Needs `$GH_TOKEN`. |

## Loop

```bash
source scripts/init-run.sh octocode    # or: gh, none
export Q=1

# gh agent — wrap every gh call:
bash scripts/gh-meas.sh search code "useState" --repo facebook/react

# octocode agent — configure the agent's MCP client to point at the proxy:
#   command: node
#   args:    [scripts/mcp-meas.mjs, octocode-mcp]
#   env:     { LOG, Q }
# Every tools/call is then logged automatically (no agent action required).

# After Q finishes:
bash scripts/record.sh "$Q" "claude-opus-4-7" answer.md
export Q=2
# ... repeat ...

node scripts/finalize.mjs "$RUN"
```

## Scoring

```bash
node scripts/judge.mjs output/<ts>-octocode output/<ts>-gh
node scripts/verify-facts.mjs   # weekly drift check
```

## Multi-run

```bash
node scripts/cross-run.mjs output/*-octocode   # medians across all octocode runs
```
