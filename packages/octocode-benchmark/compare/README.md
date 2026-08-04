# Comparisons

Each comparison folder is plain markdown:

- `README.md` — the matchup: the two arms (baseline CLI vs Octocode CLI) and their allowed surface.
- `questions/` — one file per question: `Q1.md`, `Q2.md`, … plus a `README.md` index.

Available comparisons: [Octocode vs gh](octocode-vs-gh/), [Octocode vs gh+rtk](octocode-vs-gh-rtk/), [Octocode vs ast-grep](octocode-vs-ast-grep/).

## Add a question

Create `questions/Q<n>.md` with exactly three parts — nothing else:

```markdown
# Q<n> — Short title

**id:** `unique-kebab-id`

## Question

Self-contained, objectively-checkable prompt naming the repo(s)/ref(s) or
$CORPUS path and exactly what to report.
```

No scope, budget, hints, claims, or reference — those bias the benchmark. Then add a row to `questions/README.md`.

## Add a comparison

Create `compare/octocode-vs-<baseline>/` with a `README.md` (the two arms) and a `questions/` folder. That's it.

## Running

There is no runner harness — benchmarks are run **by hand**. See the top-level [`README.md`](../README.md) and [`INSTRUCTIONS.md`](../INSTRUCTIONS.md). Tracked outcomes go to [`../results/`](../results/).
