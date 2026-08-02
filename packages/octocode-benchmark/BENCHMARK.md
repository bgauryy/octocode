# Octocode Benchmark

## TL;DR — who wins? (Research Efficiency = tokens efficiency × quality·accuracy)

**Winner metric — judging is three-dimensional: tokens × accuracy/quality × flow.**
`REQ = correctness × (quality/5) × (flow/5) / read-KB` — value density per context kilobyte. *flow* (1–5) is a dedicated trajectory grade: capability fit, minimal calls, caps lifted, cross-checks, honest Unknowns, budget discipline (scored by a separate flow judge over the 8 run-c worker reports). A verbose correct answer, a cheap wrong answer, and a lucky right answer via a chaotic trajectory all lose. Two byte accountings, both reported (their disagreement is itself a finding): **raw** = raw tool stdout (run b, single-agent); **read** = worker-self-reported bytes actually read after shell filtering (run c, independent subagents + blind judges).

| Comparison (run c) | A: corr·qual·flow | B: corr·qual·flow | **REQ B/A** |
|---|---|---|---:|
| octocode vs `gh` | 0.85 · 4.6 · 4 | 0.85 · 4.5 · 4 | **139×** |
| octocode vs `rtk`+`gh` | 1.00 · 5.0 · 4 | 0.95 · 4.5 · 4 | 0.24× |
| octocode vs `ast-grep` | 0.90 · 4.1 · 5 | 0.95 · 4.7 · 5 | 1.21× value (bytes n/r) |
| octocode vs bare POSIX | 0.85 · 4.5 · 4 | 0.95 · 4.1 · 4 | 0.25× |

| Comparison | RES raw (run b) | RES read (run c) | Correctness b / c (B vs A) | Verdict |
|---|---:|---:|---|---|
| octocode vs `gh` | **≈69×** | **≈133×** | 1.00–0.80 / 0.85–0.85 | 🏆 **octocode WIN, robust** — `gh` has no range/symbol fetch, so multi-MB whole-file pulls are structural, not solver-fixable |
| octocode vs `rtk`+`gh` | ≈11× | 0.24× | 0.95–0.90 / 0.95–1.00 | ⚖️ **TIE across 3 runs** — a disciplined `gh --jq` agent matches octocode; direction flips with accounting. Needs k≥3 + runner tokens to settle |
| octocode vs `ast-grep` | ≈7.4× | B ahead (A bytes n/r) | 1.00–0.75 / 0.95–0.90 | 🏆 **octocode** — parity on clean AST, wins identity/reachability/outline; both runs re-found the Flow mis-parse |
| octocode vs bare POSIX (react) | — | 0.25× (B more correct) | — / 0.95–0.85 | **split** — octocode more correct, filtered POSIX cheaper on read-bytes |
| octocode CLI vs MCP (self) | ≈4× read-bytes, same engine | — | 1.00–1.00 | CLI cheaper warm; MCP fewer calls — and 1 real MCP bug found |

**One sentence:** across 40 questions × 2 independent runs (2026-08-02), octocode is decisively better than plain `gh` (69–133× research efficiency) and better-or-equal everywhere else on accuracy — but a token-disciplined shell agent closes the byte gap on filtered baselines, so octocode's durable edge is **capabilities** (AST, LSP identity, outlines, bounded reads, reachability) plus raw-payload economy, not filtered-read economy.

**Honesty notes:** solver skill dominates at k=1 (run-c's octocode gh-worker lost 1.5 points to budget mismanagement, not toolchain limits); Flow-typed-JS AST unreliability is now confirmed by 3 independent solvers — top engine work item; false confidence = 0 in all 100 scored question-arms.

All 15 active tools, both surfaces (MCP + CLI), vs real alternatives and vs itself.
Docs-driven: an agent runs the checks. Two KPIs per check, judged blind against frozen oracles:

1. **Tokens** — runner-reported agent tokens (authoritative); tool-output bytes / chars÷4 as fallback.
2. **Quality** — rubric correctness `1.0/0.5/0` + judge quality `1–5` (exactness, concision, `file:line`/sha anchors).

Guardrails (untunable): false-confidence must not increase; wall-clock, turns, code-search calls reported. A no-tools control arm (C) flags contaminated questions → excluded from the correctness primary.

## Check matrix

| Check | Arms | Proves | Status |
|---|---|---|---|
| [`per-tool/`](benchmark/per-tool/) (15 docs) | octocode CLI solo | every tool works, full schema | maintained — run before any comparison |
| [`octocode-vs-gh`](benchmark/compare/octocode-vs-gh/) | `gh` CLI vs octocode | GitHub research value vs standard CLI | **WIN 2026-08-02** (1.00 vs 0.67 uncontaminated; 0.098× bytes) · **replicated run b same day**: 1.00 vs 0.80; 0.022× bytes |
| [`octocode-vs-gh-rtk`](benchmark/compare/octocode-vs-gh-rtk/) | `rtk`+`gh` vs octocode | value vs token-optimized baseline (ship-gate) | **scored 2026-08-02**: correctness TIE 0.90/0.90; 0.24× bytes; 1.4× wall-clock · **run b**: 0.95 vs 0.90 (noise); 0.112× bytes; 2.4× wall-clock |
| [`octocode-vs-ast-grep`](benchmark/compare/octocode-vs-ast-grep/) | `ast-grep` vs octocode | AST parity + beyond-AST | **scored 2026-08-02 (run b)**: 1.00 vs 0.75 — AST parity zone TIE (counts equal/attributed), beyond-AST WIN (LSP identity refs, reachability, outline); ast-grep 0.45.0 Flow mis-parse attributed (+10 false matches Q3, 75/125 fns Q8); ast-grep 3–4× faster/call, best-in-class node-extract read (Q9) · [results](benchmark/compare/octocode-vs-ast-grep/results.md) |
| [`octocode-vs-baseline-local-react`](benchmark/compare/octocode-vs-baseline-local-react/) | bare POSIX vs octocode | local research vs shell primitives | **scored 2026-08-02 (run c, subagents + blind judge)**: B more correct 0.95 vs 0.85; A cheaper on read-bytes; Flow-AST unreliability disclosed · [results](benchmark/compare/octocode-vs-baseline-local-react/results.md) |
| [`octocode-mcp-vs-cli`](benchmark/compare/octocode-mcp-vs-cli/) | MCP vs CLI surface | context-token cost per surface + data parity | **scored 2026-08-02 (run b)**: correctness TIE 1.00/1.00; **1 surface bug found** (MCP silently drops unknown fields → unfiltered junk results — CLI alias-folds the same input), no `ghCloneRepo` without `ENABLE_CLONE`, 161-entry `entrypointsResolved` L2 tax; MCP 19 calls vs CLI 33; L0 cold ~183 KB vs 4.2 KB |

Each compare suite = 10 questions. `questions.md` = solver-facing, frozen. `ground-truth.json` = judge-only.

## Results ledger (required)

Every compare suite carries a **tracked** `results.md` next to its `questions.md` — the durable record, since `output/<run>/` is gitignored. **After every scored run you MUST refresh the suite's `results.md`** with, latest run first:

1. **Time of check** (date + local time of solves/judging) and run name.
2. **Verdict line** (pre-registered decision rule applied) + provenance (SHA, versions, k, blind-or-not, oracle-verification date).
3. **Performance comparison matrix** — markdown table, one row per metric: correctness (primary + all-N), quality, **flow (trajectory grade 1–5, judged separately)**, bytes, est. tokens, calls, wall-clock, false-confidence, each with the B/A ratio column, plus the combined **REQ = correctness × quality/5 × flow/5 per read-KB**. Judging is always three-dimensional: tokens AND accuracy/quality AND flow.
4. **Per-question matrix** — correctness + bytes per arm per question, contamination flags.
5. **Conclusion** — 2–5 sentences: what won, why (capability attribution), guardrails, watch items, what the next run must fix.
6. **Prior-runs table** — append, never overwrite, so trends stay visible.

Unscored suites keep a `results.md` stub ("NOT YET SCORED" + prerequisites + time of check).

## Current results — conclusion & performance matrix (as of 2026-08-02 16:15 IDT, run compare-run-20260802-b)

| Metric | octocode vs `gh` | octocode vs `rtk`+`gh` | MCP vs CLI (self) | octocode vs `ast-grep` |
|---|---|---|---|---|
| **Verdict** | **WIN** | **TIE correctness / WIN cost** | **TIE (by design) + 1 surface bug found** | **AST parity TIE / beyond-AST WIN** |
| Correctness (primary) | **1.00 vs 0.80** (uncontaminated n=5) | 0.95 vs 0.90 (all-10, k=1 noise) | 1.00 vs 1.00 | **1.00 vs 0.75** (parity zone 1.00/1.00) |
| Quality (judge 1–5) | **4.7** vs 3.9 | **4.8** vs 4.1 | 4.8 vs 5.0 | **5.0** vs 3.9 |
| Bytes into context (B/A) | 93,905 / 4,273,866 = **0.022×** | 38,378 / 343,719 = **0.112×** | L2 ~73 KB unfiltered vs ~18 KB read | 345 KB vs 1.06 MB excl-Q5 (**0.33×**; A's Q5 json dump = 193 MB raw) |
| Tool calls (B/A) | 34 / 45 | 25 / 28 | 19 / 33 | 17 / 18 |
| Wall-clock (B/A) | 1.8× slower | 2.4× slower | n/a (per-call ≈ equal engine) | 2.3× slower (ast-grep very fast/call) |
| False confidence | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 |
| Detail | [results](benchmark/compare/octocode-vs-gh/results.md) | [results](benchmark/compare/octocode-vs-gh-rtk/results.md) | [results](benchmark/compare/octocode-mcp-vs-cli/results.md) | [results](benchmark/compare/octocode-vs-ast-grep/results.md) |

**Conclusion (ast-grep suite):** where ast-grep parses cleanly the engines agree exactly (274=274, 50=50 with identical sets, census Δ0.11%) and ast-grep is faster per call with a superb single-node extract; octocode wins everything requiring identity (LSP refs), reachability (dead exports), outlines, or Flow-typed files — ast-grep 0.45.0 `-l js` mis-parses Flow generics, producing spurious multi-line matches (attributed at `file:line`).

**Conclusion:** octocode decisively beats plain `gh` (more correct AND 45× cheaper in context); against the token-optimized `rtk`+`gh` baseline it ties on correctness while reading 9× fewer bytes — for LLM agents, where context is the binding constraint, **octocode is the better default**, at the cost of 1.8–2.4× tool wall-clock. Every correctness gap traced to a capability (AST vs text, matchString region reads, symbols outline, LSP call hierarchy, docs grounding), not luck. The MCP-vs-CLI self-eval confirmed engine parity and surfaced the run's most valuable finding: the MCP surface silently drops unknown fields (confidently-wrong results) where the CLI alias-folds them — fix queued. Open items: control arm for gh-rtk, ≥3 solvers + blind judge, re-oracle vscode-Q5, PR-metadata payload weight (the one lane where `gh --jq` is cheaper).

## Frozen corpus (all local-lane checks)

Never benchmark against this repo (it drifts). One pinned checkout:

```bash
git clone https://github.com/react/react.git packages/octocode-benchmark/context/react
git -C packages/octocode-benchmark/context/react checkout 9ceb1e7d9e20bd0302cf6ab31b038c5ec673178d
```

~1,873 Flow-typed `.js` files; gitignored; verify `rev-parse HEAD` before any run.

## Contracts

Schemas in [`benchmark/schemas/`](benchmark/schemas/): [questions-input](benchmark/schemas/questions-input.schema.json) · [solver-output](benchmark/schemas/solver-output.schema.json) · [ground-truth](benchmark/schemas/ground-truth.schema.json) · [kpi](benchmark/schemas/kpi.schema.json) (fixture: [`fixtures/compare-run-example/`](benchmark/fixtures/compare-run-example/kpi.json)). Runs write gitignored `output/<run-name>/`.

## Judge protocol

1. Oracles frozen before solvers run; verified **outside every arm** (curl api.github.com / raw / npm registry); dated — GitHub facts drift, re-verify per run.
2. Solvers never read ground truth; judge blind to arm.
3. Judge re-fetches every cited sha/PR/issue; fabricated cite = 0 + false-confidence.
4. Trajectory layer: logged calls checked against `capabilityPoint`/`expectedWorkflow` (tool + features: matchString, pagination, reviewMode, structural). Right answer without them = "answered without the tool".
5. Never edit questions/rubrics mid-run (REJECT). Evolve between runs; record corrections in ground truth.

## How to run

```bash
node ./scripts/dev-setup.mjs && yarn install   # pin local workspace build
CLI="node packages/octocode/out/octocode.js"
$CLI tools <name> --scheme --brief             # schema (source of truth)
$CLI tools <name> --queries '<json>' --compact # run a check
```

Scored run sequence:

1. Per-tool smoke — [`per-tool/README.md`](benchmark/per-tool/README.md).
2. Freeze: verify oracles (curl), pin corpus SHA + model + step budget.
3. Control arm (C) first — flags contamination.
4. Arms A/B — ≥1 solver each (≥3 for pass^k), every call logged `{cmd|tool, exit, ms, bytes}`.
5. Judge — fresh context, blind; correctness + quality + trajectory.
6. Report — `output/<run>/` per [`REPORT_TEMPLATE.md`](benchmark/compare/REPORT_TEMPLATE.md).

Method/metrics/decision rule: [`compare/README.md`](benchmark/compare/README.md). Baselines: `gh` (authed), `rtk`, `ast-grep`; `OCTOCODE_TOKEN` or gh auth for remote.

## Rules

- `--scheme` is the source of truth — fix checks that drift from it.
- Counting runs must lift caps (`maxFiles`/`itemsPerPage`/`maxMatchesPerFile`) — defaults truncate silently.
- Report dropped/timed-out/contaminated questions explicitly. Snippets are discovery, not proof.
