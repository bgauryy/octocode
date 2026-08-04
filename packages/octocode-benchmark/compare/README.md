# Comparisons

Each comparison folder is plain markdown:

- `README.md` — the matchup: the two arms (baseline CLI vs Octocode CLI) and their allowed surface.
- Questions — the three GitHub matchups all run one canonical set in [`github-questions/`](github-questions/) (a question edited there applies to all three at once); a corpus-local matchup would instead keep its own `questions/` folder with a `README.md` index.

Available comparisons: [Octocode vs gh](octocode-vs-gh/), [Octocode vs gh+rtk](octocode-vs-gh-rtk/), [Octocode vs gh+headroom](octocode-vs-gh-headroom/). Shared GitHub question set: [`github-questions/`](github-questions/).

## Add a question

Put it in the right place: a **GitHub** question goes in the shared [`github-questions/`](github-questions/) (applies to all three GitHub matchups); a **corpus-local** question (for a matchup pinned to a local checkout) goes in that matchup's own `questions/`. Create `Q<n>.md` with exactly three parts — nothing else:

```markdown
# Q<n> — Short title

**id:** `unique-kebab-id`

## Question

Self-contained, objectively-checkable prompt naming the repo(s)/ref(s) or
$CORPUS path and exactly what to report.
```

No scope, budget, hints, claims, or reference — those bias the benchmark. Then add a row to that set's `README.md` index.

## Add a comparison

Create `compare/octocode-vs-<baseline>/` with a `questions/` folder and a
`README.md` that follows the **matchup README convention** in
[`../skills/octocode-benchmark/SKILL.md`](../skills/octocode-benchmark/SKILL.md):
the two arms and their allowed surface, **how to run each arm** (prereqs, exact
allowed invocations, footprint tips), and how chars in/out are measured. A
transport/compression arm (like `gh + headroom`) also documents its wrapper, the
setup gotcha, log-based measurement, and no-re-expansion policy —
[`octocode-vs-gh-headroom/`](octocode-vs-gh-headroom/) is the worked example.

## Running

There is no runner harness — benchmarks are run **by hand**. See the top-level [`README.md`](../README.md) and [`INSTRUCTIONS.md`](../INSTRUCTIONS.md). Tracked outcomes go to [`../results/`](../results/); run scratch (measurement logs, smoke tests, local corpora) goes in a gitignored `tmp/` folder and is never committed.
