# Preflight + measurement

Load before running a matchup. A failed preflight invalidates the run. Paths are relative to
the benchmark package root (`packages/octocode-benchmark/`).

## Phase 0 — preflight (orchestrator, once)

Fast path: `bash skills/octocode-benchmark/scripts/check-prereqs.sh [octocode-version]` runs
every check and exits non-zero on any failure. What it verifies (or check by hand):

| Arm | Confirm | Command |
|---|---|---|
| octocode | CLI + live tool call | `npx octocode@<ver> --version` + probe `npx octocode@<ver> tools ghSearchRepos --queries '{"keywords":["is"],"owner":"sindresorhus","limit":1}'` |
| rtk | gh authed + rtk | `gh --version` · `gh auth status` · `rtk --version` + probe `rtk gh search repos octocode --limit 1` |
| headroom | wrapper compresses | `export HR_PY=…; ./compare/bin/preflight.py --warmup` (a `0%` ratio / `router:protected` = compression OFF → invalid) |

Also read the matchup's `compare/<matchup>/README.md` surface and confirm the primers in
`RUNNER_TOOL_CONTEXT.md`.

## Measurement (transparent, required)

Every research command runs through a wrapper that prints the real output unchanged and
counts Unicode chars **both directions** — command string (`model_out_chars`) and returned
output (`model_in_chars`) — appending one JSONL row per call; the final answer is logged as
pure model-out. Never trust a self-reported count.

**Canonical instrumentation:** thin per-arm wrappers shell the exact CLI through
`bin/instrument_command.py` — `bin/octoc` (Octocode), `bin/rtkm` (gh+RTK), `bin/ghc`
(gh+Headroom, compressed) — writing `<arm>-p<pass>-Q<n>.jsonl`. Log the final answer with
`bin/record_answer.py` (a `kind:"answer"` model-out row). Total per arm/pass/question comes
from `bin/sumlog.py --strict`; the whole campaign is checked by `bin/validate_campaign.py`.
(These live under `compare/bin/`, shared by every matchup.)

**Fallback:** an arm with no dedicated wrapper MAY use
`scripts/measure.sh <arm> Q<n> <label> -- <cmd>`; it emits the same
`model_in_chars`/`model_out_chars`/`total_chars` fields so `sumlog.py` folds it in
identically.

Next: run the phases — [`run-phases.md`](run-phases.md).
