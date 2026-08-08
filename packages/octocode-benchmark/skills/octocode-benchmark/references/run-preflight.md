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
`compare/bin/instrument_command.py` — `compare/bin/octoc` (Octocode), `compare/bin/rtkm`
(gh+RTK), `compare/bin/ghc` (gh+Headroom, compressed), `compare/bin/ghm` (plain gh, bare
baseline) — writing `<arm>-p<pass>-Q<n>.jsonl`. Log the final answer with
`compare/bin/record_answer.py` (a `kind:"answer"` model-out row). Total per arm/pass/question
comes from `compare/bin/sumlog.py --strict`; the whole campaign is checked by
`compare/bin/validate_campaign.py`. (These live under `compare/bin/`, shared by every matchup.)

**Which octocode wrapper (all under `compare/bin/`):** use `octoc-local` for the
cross-matchup comparable rollup (builds Octocode from the local monorepo — pin the
build/commit), `octoc1822` for published-CLI validation (`npx -y octocode@18.2.2`), and
**never** bare `octoc` (unpinned `npx octocode`) for a headline run. Record the exact wrapper
+ version in the report.

**Fallback:** an arm with no dedicated wrapper MAY use
`scripts/measure.sh <arm> Q<n> <label> -- <cmd>`; it emits the same
`model_in_chars`/`model_out_chars`/`total_chars` fields so `sumlog.py` folds it in
identically. Note `measure.sh` writes a per-arm aggregated `<rundir>/<arm>/calls.jsonl`, **not**
the canonical per-question `<arm>-p<pass>-Q<n>.jsonl`, so point `sumlog.py` at that
`calls.jsonl` — `validate_campaign.py` globs the per-question logs and does not consume it.
Prefer the `compare/bin/` wrappers for a headline campaign.

Next: run the phases — [`run-phases.md`](run-phases.md).
