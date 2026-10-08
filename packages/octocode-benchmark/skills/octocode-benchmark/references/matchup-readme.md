# Matchup (arm) README convention

Load when authoring or reviewing a `compare/<matchup>/README.md` — the doc a fresh runner
reads to execute **one baseline arm** without guessing. A vague arm README silently biases
the run. Octocode (the anchor arm) is defined centrally in `RUNNER_TOOL_CONTEXT.md`; each folder here
defines the **baseline** it pits against Octocode.

Every arm README must include:

1. **Arm definition** — this baseline and its allowed **read-only** GitHub surface (the
   exact families/verbs; no mutations).
2. **How to run it** — prerequisites/versions to confirm, the exact allowed invocations
   with examples, and footprint tips (targeted reads, snippets, minimal `--json`). Pin any
   corpus SHA or tool version that changes results.
3. **Leanest-path expectation** — restate the fairness rule: no whole-tree/whole-file dumps
   where a targeted read or search answers. This baseline is compared against Octocode on
   equal, leanest paths.
4. **Measurement** — how this arm's characters are captured in both directions for
   `SCORING.md`: model-in (output pulled into context) + model-out (commands/args + final
   answer) = `total_chars`, from an instrumented log, never self-reported.

## Transport / compression arms

`rtk` and `headroom` are **not extra research sources** — they only reshape `gh` output, so
the arm's GitHub surface stays identical to plain `gh`. Such a README must additionally
state:

- the exact wrapper command;
- the setup gotcha that silently disables compression if missed, and how to detect it (a 0%
  ratio / `router:protected` means the measurement is invalid — rerun);
- that chars are read from an instrumented log, never the runner's self-report;
- the no-re-expansion (no CCR retrieve) policy that keeps model-in chars honest.

Worked example: `compare/octocode-vs-gh-headroom/README.md` (paths relative to the benchmark
package root).
