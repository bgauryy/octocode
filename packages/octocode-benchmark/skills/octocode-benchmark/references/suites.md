# Suite Parameters

Load when doing preflight for any suite — has every suite-specific value you
need to brief runners, verify corpora, and compute KPIs.

> **Both GitHub suites use the 14-question `research-v2` bank** — the single
> canonical bank in the tree (`questions/github/research-v2/`, Q1–Q14). There is
> no `top10` bank; any older reference to `research-v2-top10` is stale. Score all
> 14; the two contaminated rows (Q6, Q10) stay visible but are excluded from the
> primary correctness mean per the bank oracle.

## Suite 1 — octocode-vs-gh

| Parameter | Value |
|---|---|
| Suite path | `compare/octocode-vs-gh/` |
| Bank path | `questions/github/research-v2/` |
| `questionBankHash` (sha256 of `questions.md`) | `f8cc969494755bfb2fe57bdeadc9eaac8ed5c67f5022af5919adfc73ea9643fe` |
| Questions | Q1–Q14 (14 questions; Q6, Q10 contaminated → excluded from primary mean) |
| Arm A | `gh` CLI only |
| Arm B | Octocode MCP remote GitHub |
| Corpus | Remote GitHub (no local clone) |
| Results ledger | `results/octocode-vs-gh.md` |
| Oracle status | `VERIFIED_WITH_REVERIFICATION_CONTRACT` |

Preflight: `shasum -a 256 questions/github/research-v2/questions.md`, confirm it
matches `questionBankHash` above (also stored in `ground-truth.json` under
`verification.questionBankHash`), then re-verify all entries with
`reverifyBeforeRun: true` against live GitHub before freezing.

## Suite 2 — octocode-vs-gh-rtk

| Parameter | Value |
|---|---|
| Suite path | `compare/octocode-vs-gh-rtk/` |
| Bank path | `questions/github/research-v2/` (SAME bank as suite 1) |
| `questionBankHash` (sha256 of `questions.md`) | `f8cc969494755bfb2fe57bdeadc9eaac8ed5c67f5022af5919adfc73ea9643fe` |
| Questions | Q1–Q14 (14 questions, identical to suite 1) |
| Arm A | `gh` CLI + `rtk` output shaping only (see rtk provenance + fairness rules below) |
| Arm B | Octocode MCP remote GitHub (same allowlist as suite 1) |
| Corpus | Remote GitHub |
| Results ledger | `results/octocode-vs-gh-rtk.md` |
| Oracle status | `VERIFIED_WITH_REVERIFICATION_CONTRACT` |

**What `rtk` is:** a third-party CLI that filters/reshapes `gh` stdout to reduce
tokens; it adds **no new research source**. It is not part of this repo. Record
the exact binary and version in `manifest.md` (`baselines.rtk`) — last observed
`rtk 0.41.0` (with `gh 2.76.2`). Verify the constraint before accepting any Arm A
answer.

**Fairness rules (HARD — remove the solver-discipline confound):**
- Any Arm A tool call whose `rawBytes` exceeds **50 KB** MUST be piped through
  `rtk` before the payload reaches the solver context. A trial that reads a raw
  `gh` payload over 50 KB unfiltered is an **invalid trial** (`taskStatus:
  invalid`) and must be re-run — it measures solver discipline, not the tool.
- The per-question `maxToolCalls` in the bank is a **hard cap**: a trial that
  exceeds it is `taskStatus: invalid` and excluded from aggregates (re-run).
- These rules apply symmetrically — Arm B trials that blow the hard call cap are
  equally invalid.

## Suite 3 — octocode-vs-ast-grep

| Parameter | Value |
|---|---|
| Suite path | `compare/octocode-vs-ast-grep/` |
| Bank path | `questions/local-code/ast-grep-react-v2/` |
| `questionBankHash` (sha256 of `questions.md`) | `9ba308e992cb2a6eb56d4a247b0c2e3cd5df3883282d34f2524053f2c54b5259` |
| Questions | Q1–Q10 (10 questions) |
| Arm A | `ast-grep` CLI (local) |
| Arm B | Octocode CLI (local) |
| Corpus | Pinned React checkout (see below) |
| Results ledger | `results/octocode-vs-ast-grep.md` |
| Oracle status | `VERIFIED_WITH_REVERIFICATION_CONTRACT` |

**Corpus setup (required before any local run):**
```bash
git clone https://github.com/facebook/react.git packages/octocode-benchmark/context/react
git -C packages/octocode-benchmark/context/react checkout 9ceb1e7d9e20bd0302cf6ab31b038c5ec673178d
# Verify:
git -C packages/octocode-benchmark/context/react rev-parse HEAD
# Must output: 9ceb1e7d9e20bd0302cf6ab31b038c5ec673178d
```

The checkout is gitignored. If it moved, re-seed the ground truth before running.

## Cross-suite notes

- Suites 1 and 2 share the same bank — both must use the identical frozen `questions.md`. If one suite runs with a drifted copy, the comparison is invalid.
- Suite 3 is independent — different bank, different capability domain (local structural search vs remote GitHub research).
- Running all three suites in one report requires three separate KPI rows in `kpi.json`.
- Contamination is per-question per-suite — the same question can be contaminated in one suite run and clean in another depending on the model.

## mutable facts to re-verify before every GitHub suite run

These drift and must be independently verified (record resolved values in manifest.md):
- All branch heads (`canary`, `unstable`, default branches) — record resolved SHA + UTC
- PR states (Q4, Q14) — record state at verification time
- Language byte totals (Q2, Q10, Q11) — record observed totals
- File paths and line anchors for any `reverifyBeforeRun: true` question
