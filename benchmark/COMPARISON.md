# Benchmark Comparison Methodology

This benchmark suite compares research tools by the answer they let an agent produce, not by raw command speed alone. A tool wins only when it preserves factual quality and research depth while spending fewer measured characters.

---

## Document Roles

| File | Job |
|---|---|
| `benchmark/README.md` | Folder map, runbook, output contract |
| `benchmark/COMPARISON.md` | Methodology, scoring rules, character/token policy |
| `benchmark/prompts/octocode-researcher.md` | Paste-ready instructions for Octocode researcher agents |
| `benchmark/prompts/rtk-gh-researcher.md` | Paste-ready instructions for RTK + gh researcher agents |
| `benchmark/prompts/judge.md` | Paste-ready instructions for judge agents |
| `benchmark/questions/README.md` | Question-bank-specific setup, allowed tools, and caveats |

---

## Tool Comparison Map

| Comparison | Question set | Agents | Claim it can support |
|---|---|---|---|
| Full code research | `benchmark/questions/nextjs.md` | `octocode` vs `rtk-gh` | End-to-end research quality across GitHub, local clone work, package registry lookup, exhaustive search, and LSP-style symbol questions. |

Use `rtk-gh` as a single agent when the claim is about RTK local filtering plus raw GitHub CLI coverage. If a future run compares bare `gh` separately, keep it as its own run directory under `benchmark/output/gh`.

`benchmark/questions/*.md` files are question banks. They become runnable through `benchmark/scripts/init-run.sh`; set `QUESTIONS_FILE` when using a non-default question bank.

---

## Question Design

A good benchmark question has one independently verifiable answer and probes a real tool capability gap.

Use questions that require at least one of:

- exhaustive search with counts and all file paths
- targeted large-file reads with exact symbols or sections
- source comments or doc comments that may be stripped by filtering tools
- PR archaeology: body text, inline review comments, labels, commits, changed files
- directory or metadata enumeration
- package registry facts outside GitHub CLI scope
- LSP-style definitions, references, callers, or type information

Avoid questions that can be answered from memory, README summaries, or subjective interpretation. Mark changing questions with `[drift]`, especially current versions, download counts, stars, recent PRs, and recently updated repositories.

---

## Character And Optional Token Measurement

The canonical ruler is:

```text
total_chars_to_answer = sum(in_chars + out_chars) across every metered call for the question
```

Why chars are canonical:

- deterministic across machines and models
- available for CLI tools that do not expose model token counters
- auditable from `log.jsonl`
- fair to tools that require multiple follow-up calls

Approximate tokens are display-only:

```text
approx_tokens = ceil(total_chars_to_answer / 4)
```

Actual LLM tokens should be reported when a runner records them, for example `lm_tokens_in + lm_tokens_out` in `q<n>.json`. Actual tokens never replace the character ruler unless every compared agent has equivalent token accounting.

Count what the research agent sees:

| Data | Count it? | Reason |
|---|---|---|
| Tool input payload or argv | Yes | It is prompt/context the agent created or chose. |
| Tool stdout returned to the agent | Yes | It becomes model context. |
| Tool stderr returned as diagnostic text | Yes for `gh`/`rtk`; no for Octocode wrapper diagnostics | Existing wrappers follow this convention. Keep it consistent per suite. |
| One-time schema or tool-context loading | Count only if it is part of the measured run; otherwise record as `init_chars` and amortize. |
| Judge fact-checking calls | No | The judge is outside researcher runs. |
| Wall-clock time | No winner impact | Report only as context. |

For the current CLI-wrapper runs, there is no MCP session schema-loading cost. If a future MCP-server benchmark includes a startup tool-list or schema dump, record it separately as init cost and amortize it across answered questions.

---

## Research Quality Measurement

Score each answer with two judge-assigned axes:

| Axis | Range | Meaning |
|---|---:|---|
| `Q` quality | 0–3 | Factual correctness of load-bearing answer facts. |
| `D` depth | 0–3 | Evidence quality: citations, exact lines, quotes, cross-checks, and completeness. |
| `T` turns | raw count | Metered tool calls or model turns used for the question. |

Composite:

```text
research_score = Q * D
tradeoff_score = research_score / max(total_chars_to_answer / 1000, 0.01)
turns_per_point = T / max(Q, 0.5)
```

Interpretation rules:

- Highest `research_score` means best answer quality, ignoring cost.
- Highest `tradeoff_score` means best quality-adjusted cost.
- A cheap answer with materially lower `Q` or `D` is an efficiency win, not a clean research win.
- Within 5% `tradeoff_score`, call a tie.
- Exclude `[drift]` questions from main totals; report them separately.
- Any score below 3 needs a specific missing or wrong fact, not a vague "incomplete" note.

Clean win threshold:

```text
clean_win = best tradeoff_score AND Q within 0.5 of best Q AND D within 0.5 of best D
```

If this threshold fails, the verdict must explicitly say what quality or depth was traded away.

---

## Minimum Valid Run

A publication-quality comparison needs:

- every command routed through the suite's metering wrapper
- one `q<n>.md` answer and one `q<n>.json` metrics file per question
- `log.jsonl`, `output.md`, and `summary.json` for each agent
- exact tool versions, model IDs, auth source, benchmark commit SHA, and retrieval dates
- judge notes for every `Q < 3` or `D < 3`
- at least three repeated runs when stochastic agent behavior is being compared

Use this generic scorer after the judge writes a quality/depth JSON file:

```bash
node benchmark/scripts/score-comparison.mjs \
  --questions benchmark/questions/nextjs.md \
  --scores benchmark/output/quality-depth.json \
  --markdown \
  octocode=benchmark/output/octocode \
  rtk-gh=benchmark/output/rtk-gh
```

The scorer performs arithmetic only. It does not judge factual correctness.
