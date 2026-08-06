# Matchup README convention

Load when authoring or reviewing a `compare/<matchup>/README.md` — the doc a fresh runner reads to execute one arm without guessing. Why: a vague arm README silently biases the run.

Every matchup README must include:

1. **Arms table** — the two arms and their allowed (read-only) surface.
2. **How to run Arm A** — the baseline tool: prerequisites/versions to confirm, the exact allowed invocations with examples, and footprint tips. Pin any corpus SHA or tool version that changes results.
3. **How to run Arm B** — the `npx octocode tools …` invocation.
4. **Measurement** — how each arm's characters are captured in both directions for `SCORING.md`: model-in (output pulled into context) + model-out (commands/args the model wrote + final answer) = `total_chars`.

## Transport / compression arms

`rtk` and `headroom` are **not extra research sources** — they only reshape `gh` output, so Arm A's GitHub surface stays identical to plain `gh`. Such a README must additionally state:

- the exact wrapper command;
- the setup gotcha that silently disables compression if missed — and how to detect it (a 0% ratio / `router:protected` means the measurement is invalid, rerun);
- that chars are read from an instrumented log, never the runner's self-report;
- the no-re-expansion (no CCR retrieve) policy that keeps model-in chars honest (model-out is the literal command + answer, always exact).

Worked example: `compare/octocode-vs-gh-headroom/README.md` (paths here are relative to the benchmark package root).
