# @octocodeai/octocode-benchmark

Plain-markdown, run-by-hand CLI research benchmarks. Each question is answered by two tools (a baseline CLI and the Octocode CLI) and graded by a third, independent reviewer.

**No harness, no JSON, no schemas.** Everything is markdown you can read and edit.

```
compare/
  github-questions/         the 20 shared GitHub questions — ONE canonical copy
  octocode-vs-gh/           README.md            (runs github-questions/)
  octocode-vs-gh-rtk/       README.md            (runs github-questions/)
  octocode-vs-gh-headroom/  README.md + bin/     (runs github-questions/)
results/                    finished write-ups (one per run)
tmp/                        run scratch — logs, corpora (gitignored, never committed)
```

- **A comparison** = a `README.md` (the two arms) + a set of questions — the GitHub matchups share [`compare/github-questions/`](compare/github-questions/); a corpus-local matchup would keep its own `questions/`.
- **A question** = one `Q<n>.md` with exactly a title, an `id`, and the `## Question` — no scope, hints, claims, or answer.
- The Octocode arm always runs as `npx octocode tools <tool> …` (no MCP, no monorepo entrypoint).

## The flow

Three separate agents per question, each working alone: **Runner A** (baseline CLI) and **Runner B** (`npx octocode tools …`) answer the same question blind to each other, then a **Grader** researches independently and scores both with tool names hidden. Keeping the roles separate and blind is what makes the numbers trustworthy — the full design and rationale live once in **[BENCHMARK.md](BENCHMARK.md)**.

## Run one

1. Pick a comparison and open one question from its set (`compare/github-questions/` for the GitHub matchups, or the matchup's own `questions/`).
2. Runner A and Runner B each answer, recording every command's characters (model-out), its output size pulled in (model-in), and the final answer's characters (model-out) — the scored total is **model-in + model-out**.
3. The grader independently establishes the facts, then grades both answers against them.
4. Roll the questions up into one write-up in [`results/`](results/) — a per-question table and a summary of all.

See [INSTRUCTIONS.md](INSTRUCTIONS.md) for the step list,
[RUNNER.md](RUNNER.md) and [RUNNER_TOOL_CONTEXT.md](RUNNER_TOOL_CONTEXT.md) for
runner packets, [JUDGING.md](JUDGING.md) for grading, [SCORING.md](SCORING.md)
for measurement, and [REPORT_TEMPLATE.md](REPORT_TEMPLATE.md) for reports.

## Add a question

Pick the set: a **GitHub** question goes in the shared [`compare/github-questions/`](compare/github-questions/) (it applies to all three GitHub matchups at once); a **corpus-local** question goes in that matchup's own `questions/`. Create `Q<n>.md` (next number) with exactly three parts:

```markdown
# Q<n> — Short title

**id:** `unique-kebab-id`

## Question

One self-contained, objectively-checkable prompt. Name the repo(s)/ref(s) or
`$CORPUS` path, and say exactly what to report. No hints, no approach, no answer.
```

Then add its row to that set's `README.md` index. That's the whole process — no scripts, no JSON. Good questions have a single correct, verifiable answer a grader can confirm from primary evidence.
