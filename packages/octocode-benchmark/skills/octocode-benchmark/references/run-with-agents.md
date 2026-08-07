# Running the campaign with isolated agents

Load when actually executing a matchup. Concrete recipe for the 4-phase **pairwise-matchup**
flow whose design lives in [`BENCHMARK.md`](BENCHMARK.md). A matchup pits the **anchor**
(`octocode`) against **one baseline** (`rtk`, `headroom`, or `gh`); repeat per baseline and
≥ 3 passes.

Non-negotiable isolation: a fresh agent per (question, arm, pass), one judge per question, no
shared transcripts, no answer key.

Two steps, load in order:

- [`references/run-preflight.md`](run-preflight.md) — Phase 0 preflight + the measurement/instrumentation contract (run once; a failure invalidates the run).
- [`references/run-phases.md`](run-phases.md) — Phases 1–3: spawn runners, build the blind packet + judge, then aggregate the outputs.
