# Comparisons

The benchmark runs as **pairwise matchups**: Octocode (the anchor) is compared against two
`gh`-based baselines on the same questions, then blind-judged. Each folder here **defines
one baseline arm**; Octocode is the constant anchor across all of them.

- `octocode-vs-gh/` — baseline: **plain `gh`**.
- `octocode-vs-gh-rtk/` — **`gh` + RTK** (transport/filter; not an extra source).
- `octocode-vs-gh-headroom/` — **`gh` + Headroom** (compression; `bin/` wrappers).

Each folder's `README.md` gives that arm's exact allowed read-only surface and how its
characters are measured. All GitHub arms run one canonical question set in
[`github-questions/`](github-questions/) — edit a question there and every arm sees it. A
corpus-local matchup keeps its own `questions/`.

Flow, phases, and fairness rule: [`BENCHMARK.md`](../skills/octocode-benchmark/references/BENCHMARK.md) ·
[`INSTRUCTIONS.md`](../skills/octocode-benchmark/references/INSTRUCTIONS.md).

## Add a question

GitHub question → shared [`github-questions/`](github-questions/) (applies to all arms);
corpus-local → that matchup's own `questions/`. Create `Q<n>.md` with exactly three parts:

```markdown
# Q<n> — Short title

**id:** `unique-kebab-id`

## Question

Self-contained, objectively-checkable prompt naming the repo(s)/ref(s) or
$CORPUS path and exactly what to report.
```

No scope, budget, hints, claims, or reference — those bias the benchmark. Add a row to that
set's `README.md` index.

## Add an arm

Create `compare/octocode-vs-<baseline>/README.md` following the **matchup README
convention** in [`../skills/octocode-benchmark/references/matchup-readme.md`](../skills/octocode-benchmark/references/matchup-readme.md):
its allowed read-only surface, exact invocations, the wrapper that measures its chars, and
any version/SHA to pin.
