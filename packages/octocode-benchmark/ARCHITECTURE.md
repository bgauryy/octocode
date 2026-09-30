# Benchmark architecture

`@octocodeai/octocode-benchmark` is a private, source-only eval workspace. It measures how a Claude agent researches code with Octocode compared with the same agent using `rg` and `gh`. It is not a production dependency and does not reimplement tool policy: the Octocode arm runs the real MCP server build.

## Data flow

```text
questions.json ──► run.mjs ──► results/<run-id>/<qid>/<arm>/pass<k>/  (stream, answer, run.json)
                                   │
                    judge.mjs ◄────┘  scrubbed answers as X/Y, both orders, tie-break
                                   │
                                   ▼
                  report.mjs ──► summary.json, report.md, docs/BENCHMARKS.md
```

## Invariants

- Solvers get only the question, the corpus paths, and a neutral request for evidence. Arm-specific coaching, answer keys, judge output and earlier attempts never reach them.
- Both arms use the same model, prompt, turn limit and timeout. Their only difference is the tool configuration in `run.mjs`.
- A run records the hashes of its questions, harness, MCP config and server build. The harness refuses to resume a run ID after any of them changes; a corrected harness means rerunning both arms.
- Every published number comes from `summary.json` through `report.mjs`. `docs/BENCHMARKS.md` is generated, not hand-edited.
- Raw streams stay local (gitignored). Unresolved judge verdicts are reported and excluded from correctness totals, never dropped silently.
