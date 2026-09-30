# Benchmark architecture

`@octocodeai/octocode-benchmark` is a private, source-only eval workspace. It measures how a Claude agent researches code under different worker definitions (instruction doc + tool profile). It is not a production dependency and does not reimplement tool policy: the Octocode worker runs the real local MCP server build.

## Data flow

```text
questions/questions.json ─┐
workers/<id>/{WORKER.md,profile.json} ─► run.mjs ──► results/<run-id>/runs/<qid>/<worker>/ (stream*, answer, run.json)
                                                        │
references/<qid>.md (evaluator-only) ──► judge.mjs ◄────┘  blinded X/Y, both orders, tie-break
                                                        │
                                                        ▼
                                        report.mjs ──► summary.json, REPORT.md
```

## Invariants

- The harness is generic over `workers/`: a worker differs only by its doc and profile.
- Solvers get their worker doc, the question, and (for local questions) the corpus path. References, judge output and earlier attempts never reach them; `--add-dir` exposes only the corpus.
- A run records hashes of the harness, questions, worker docs/profiles and the MCP build; it refuses to resume after any of them changes.
- Raw streams stay local (gitignored). Malformed judge output is a grader error, reported, never silently dropped.
