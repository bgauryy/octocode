# @octocodeai/octocode-benchmark

Plain-markdown, run-by-hand CLI research benchmarks. Each question is answered by two tools (a baseline CLI and the Octocode CLI) and graded by a third, independent reviewer.

**No harness, no JSON, no schemas.** Everything is markdown you can read and edit.

```
compare/
  octocode-vs-gh/        README.md + questions/Q1.md, Q2.md, …
  octocode-vs-gh-rtk/    README.md + questions/Q1.md, Q2.md, …
  octocode-vs-ast-grep/  README.md + questions/Q1.md, Q2.md, …
results/                 finished write-ups (one per run)
```

- **A comparison** = a `README.md` (the two arms) + a `questions/` folder.
- **A question** = one `Q<n>.md` with exactly a title, an `id`, and the `## Question` — no scope, hints, claims, or answer.
- The Octocode arm always runs as `npx octocode tools <tool> …` (no MCP, no monorepo entrypoint).

## The flow (three separate people/agents per question)

```
QUESTION
  ├─ Runner A  answers with the baseline CLI only          → write it down
  ├─ Runner B  answers with `npx octocode tools …` only    → write it down
  └─ Grader    reads both answers blind, researches, grades → write it down
```

Keep the two runners unaware of each other and of the grader. Give both the same question and budget; only their CLI differs. The grader checks each answer on its own evidence before comparing.

## Run one

1. Pick a comparison and open a `questions/Q<n>.md`.
2. Runner A and Runner B each answer, recording every command and its output size in **characters**.
3. The grader independently establishes the facts, then grades both answers against them.
4. Roll the questions up into one write-up in [`results/`](results/) — a per-question table and a summary of all.

See [INSTRUCTIONS.md](INSTRUCTIONS.md) for the step list, [RUNNER.md](RUNNER.md) / [JUDGING.md](JUDGING.md) for the two roles, [SCORING.md](SCORING.md) for what to measure, and [REPORT_TEMPLATE.md](REPORT_TEMPLATE.md) for the write-up shape.

## Add a question

Create `questions/Q<n>.md` (next number) with exactly three parts:

```markdown
# Q<n> — Short title

**id:** `unique-kebab-id`

## Question

One self-contained, objectively-checkable prompt. Name the repo(s)/ref(s) or
`$CORPUS` path, and say exactly what to report. No hints, no approach, no answer.
```

Then add its row to `questions/README.md`. That's the whole process — no scripts, no JSON. Good questions have a single correct, verifiable answer a grader can confirm from primary evidence.
