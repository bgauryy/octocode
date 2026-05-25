# GitHub Research Benchmark

46 questions, one toolset per agent, deterministic metering. Measures cost on the same ruler regardless of toolset, then scores answer quality against [`EXPECTED_FACTS.md`](./EXPECTED_FACTS.md).

This is the single source of truth. Operators read this file; researcher agents read the §Agent Instructions block below; judge agents read the Agent C block.

## Agents

| Slug | Toolset | Metering |
|------|---------|----------|
| `octocode` | Octocode MCP tools only | `scripts/mcp-meas.mjs` — transparent stdio proxy, every `tools/call` auto-logged |
| `gh` | `gh` CLI only | `scripts/gh-meas.sh` — wrapper; agent must use it on every call |
| `none` | Training knowledge only | nothing to log (baseline) |

[`QUESTIONS.md`](./QUESTIONS.md) is tool-agnostic — nothing else is communicated to the answering agent.

## Agent prompts

Two paste-ready prompts live in [`prompts/`](./prompts/):

| Prompt | Used by | What it does |
|---|---|---|
| [`prompts/researcher.md`](./prompts/researcher.md) | The two researcher agents (one per toolset) | Single prompt that branches on a `<TOOLSET>` placeholder. Operator sets it to `octocode` or `gh` before pasting. The agent then walks Q1→QN sequentially using only its assigned toolset, recording every call via the metering scripts. |
| [`prompts/judge.md`](./prompts/judge.md) | The judge agent | Reads both completed runs + `EXPECTED_FACTS.md`, runs `judge.mjs` as a heuristic starting point, overlays semantic per-Q evaluation (quality, efficiency, honesty), writes the comparison to `output/summary.md`. |

**Order of operations:**

1. Paste `prompts/researcher.md` to agent A with `<TOOLSET>=octocode`. Wait for the run to finish (`finalize.mjs` writes `summary.json`).
2. Paste `prompts/researcher.md` to agent B with `<TOOLSET>=gh`. Wait for the run to finish.
3. Paste `prompts/judge.md` to agent C with the two run dir paths. It writes `output/summary.md`.

The prompts are self-contained — the agents do not need to read this README to function. This README is the operator/reviewer view.

## What is and is not deterministic

| Aspect | Deterministic? |
|--------|---------------|
| `in_chars`, `out_chars`, `calls` per Q | **Yes** — given the same log.jsonl, every downstream artifact is byte-identical |
| `judge.mjs` scoring | **Yes** — same answer + same facts → same score |
| `validate-pipeline.mjs` | **Yes** — asserts the above |
| `tool_elapsed_ms`, `q_elapsed_ms`, `reasoning_ms` | **No** — wall clock; reported but excluded from validate-pipeline |
| `ts`, `Recorded` timestamp | **No** — wall clock; never used by scoring |
| Agent behaviour (the LLM) | **No** — different research paths per run; quantified by `report-variance.mjs`, not asserted |

For golden-file testing of `output.md`, set `DETERMINISTIC=1` before `record.sh` — the `Recorded` field becomes `—` and `q_elapsed_ms` becomes `0`, so output bytes become reproducible.

## Timing dimensions (two clocks)

Per question we record two wall-clock measurements:

| Field | What it measures | When captured |
|---|---|---|
| `tool_elapsed_ms` | Σ wall time of individual tool calls (octocode: per `tools/call` round-trip; gh: per `gh` subprocess). | Per call, in `log.jsonl` (also exposed as `elapsed_ms`). Summed by `aggregate.mjs`. |
| `q_elapsed_ms` | Wall clock from `set-q.sh <n>` to `record.sh <n>`. Captures the agent's TOTAL time on Q: tool waits + reasoning + answer writing. | Per question, set by `set-q.sh` (writes `$RUN/.q-start`) and finalised by `record.sh`. |
| `reasoning_ms` (derived) | `max(0, q_elapsed_ms − tool_elapsed_ms)`. Approximates time the LLM spent thinking between tool calls. | Computed by `finalize.mjs`. |

A cheap-on-chars agent that takes 5 minutes per question is not necessarily cheap on compute — `q_elapsed_ms` is the honest end-to-end clock. Reasoning time is the largest hidden cost and is now visible.

## Metering ruler — payload only, both agents

Same definition for both:

| | octocode | gh |
|---|---|---|
| `in_chars`  | codepoints of `JSON.stringify(params.arguments)` | codepoints of the argv tail (excluding the literal `gh ` prefix) |
| `out_chars` | codepoints of `result.content[].text` concatenated | codepoints of stdout + stderr |

JSON-RPC envelope bytes and the `gh ` command word are excluded so neither agent is penalised for transport overhead.

### MCP init context (octocode-only baseline)

Beyond per-tool-call payloads, the octocode agent pays a **one-time per-session cost**: when the MCP client connects, it sends `initialize` and `tools/list` requests; the server responds with its instructions plus the full JSON schema for every tool. All of that is loaded into the agent's context window — real LLM token cost that gh users don't pay.

`mcp-meas.mjs` logs these responses with `q=0` and `cmd="_initialize"` / `cmd="_tools/list"`. `finalize.mjs` exposes the total as `mcp_init` in `summary.json` and surfaces it in `output.md`. `judge.mjs`'s "Total chars" column includes it.

| Agent | MCP init chars |
|---|---|
| octocode | sum of `_initialize` + `_tools/list` response payloads (typically 10–30 KB) |
| gh | 0 (gh CLI does not push schemas into the LLM context) |

This is what makes the agent comparison honest. Without it, octocode would appear cheaper than it actually is.

## Operator commands

```bash
# 1. Initialise a session (both agents share one session folder)
source docs/dev/benchmark/github/scripts/init-run.sh octocode  # creates output/<ts>/  + output/<ts>/octocode/
source docs/dev/benchmark/github/scripts/init-run.sh gh         # reuses $SESSION      + output/<ts>/gh/

# 2. Per-question loop for each agent (46 questions each — sequential within each agent)
bash docs/dev/benchmark/github/scripts/set-q.sh <n>
bash docs/dev/benchmark/github/scripts/record.sh <n> "<model>" /tmp/answer.md
# record.sh writes $RUN/q<n>.md + $RUN/q<n>.json  (flat, no subdirs)

# 3. Finalise each agent run
node docs/dev/benchmark/github/scripts/finalize.mjs "$SESSION/octocode"
node docs/dev/benchmark/github/scripts/finalize.mjs "$SESSION/gh"

# 4. Score — paste prompts/judge.md to the judge agent with the two run paths
#    The judge agent reads both runs + EXPECTED_FACTS.md and writes $SESSION/summary.md

# 5. Multi-run analysis (run each agent ≥3× across sessions)
node docs/dev/benchmark/github/scripts/cross-run.mjs output/*/octocode      # medians
node docs/dev/benchmark/github/scripts/report-variance.mjs output/*/octocode # CV / stability
node docs/dev/benchmark/github/scripts/validate-pipeline.mjs output/*/octocode  # metering regression
```

## Scoring model

Evaluation is done by the judge agent (see `prompts/judge.md`), not a script. The judge reads every answer in `q<n>.md` and `EXPECTED_FACTS.md` and scores semantically (0–3 per question). This handles paraphrasing, partial credit, and nuanced hallucinations that token-matching cannot.

**Drift questions** (heading suffix `[drift]` in `EXPECTED_FACTS.md` — Q12, Q13, Q14, Q16, Q26, Q28, Q40, Q41, Q46) are scored loosely and reported separately, excluded from the primary verdict total — their facts change between runs.

**Pareto winner per Q:** higher quality wins; on tie, lower total chars (in+out from `q<n>.json`) wins.

**Authoring rules for EXPECTED_FACTS.md:**
- Write facts as plain prose bullets. Be specific: name the file, function, or behaviour the agent must find.
- Add `[drift]` to the heading if any fact references a value that changes between runs.

## Common mistakes (read before running)

| Mistake | Symptom | Fix |
|---|---|---|
| Forgot `set-q.sh` before tool call | `record.sh` warns "sentinel says Q=N but you're recording Q=M" | Always `set-q.sh` first |
| Bare `gh` instead of `gh-meas.sh` | `aggregate.mjs` reports zero rows | Re-do the Q via the wrapper |
| Octocode agent ran Qs in parallel | `q` field in log is wrong; cross-Q metric leak | Strictly sequential — finish Q<n> fully before Q<n+1> |
| Agent read `EXPECTED_FACTS.md` | Run is no longer blind | Discard the run, restart |
| Answer written but `record.sh` skipped | `q<n>.md` missing in finalize | Run `record.sh` per question |
| Comparing runs of different agents in `validate-pipeline.mjs` | Hard error (intentional) | Compare same-agent only |

## Layout

```
github/
├── README.md                ← this file (operator / reviewer view)
├── QUESTIONS.md             ← researcher input (tool-agnostic)
├── EXPECTED_FACTS.md        ← judge answer key (DO NOT show researchers)
├── prompts/                 ← paste-ready agent prompts
│   ├── researcher.md        one prompt; operator sets <TOOLSET> = octocode | gh
│   └── judge.md             reads both runs; writes <session>/summary.md
├── scripts/                 ← see scripts/README.md for the API reference
│   ├── init-run.sh          create session + agent run dir (exports $SESSION, $RUN, $LOG)
│   ├── set-q.sh             advance question pointer + .q-start
│   ├── mcp-meas.mjs         octocode metering proxy (logs init + tools/list + tools/call)
│   ├── gh-meas.sh           thin shim → gh-meas.mjs
│   ├── gh-meas.mjs          gh metering wrapper
│   ├── octo-meas.sh         manual fallback (last resort)
│   ├── chars.mjs            codepoint counter
│   ├── aggregate.mjs        per-Q log → totals (fail-loud)
│   ├── record.sh            writes q<n>.md + q<n>.json flat in $RUN
│   ├── finalize.mjs         run → output.md + summary.json (incl. mcp_init)
│   ├── cross-run.mjs        median across N runs
│   ├── report-variance.mjs  CV / stability across N runs
│   ├── validate-pipeline.mjs   metering-pipeline regression check
│   └── call-tool.mjs        one-shot MCP smoke-test client
└── output/
    └── <session-ts>/          one directory per benchmark session (one comparison)
        ├── summary.md           written by the judge agent (prompts/judge.md)
        ├── octocode/            octocode agent run
        │   ├── log.jsonl          per-call metering log (q=0 rows = MCP init)
        │   ├── .current-q         Q routing sentinel
        │   ├── .q-start           epoch ms for q_elapsed_ms
        │   ├── .q-count           total Q count
        │   ├── q1.md  q1.json     per-Q answer + metrics (flat, no subdirs)
        │   ├── q2.md  q2.json  …
        │   ├── output.md          finalize human summary
        │   └── summary.json       finalize machine sidecar (incl. mcp_init)
        └── gh/                  gh agent run (same layout, no mcp_init)
            ├── log.jsonl
            ├── q1.md  q1.json  …
            └── output.md  summary.json
```

## Conflict of interest

Designed by the Octocode team. Questions were chosen for code-research realism; independent replication with a different question set is welcomed.
