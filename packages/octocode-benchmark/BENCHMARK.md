# Octocode Benchmark

All 15 active tools, both surfaces (MCP + CLI), vs real alternatives and vs itself.
Docs-driven: an agent runs the checks. Two KPIs per check, judged blind against frozen oracles:

1. **Tokens** — runner-reported agent tokens (authoritative); tool-output bytes / chars÷4 as fallback.
2. **Quality** — rubric correctness `1.0/0.5/0` + judge quality `1–5` (exactness, concision, `file:line`/sha anchors).

Guardrails (untunable): false-confidence must not increase; wall-clock, turns, code-search calls reported. A no-tools control arm (C) flags contaminated questions → excluded from the correctness primary.

## Check matrix

| Check | Arms | Proves | Status |
|---|---|---|---|
| [`per-tool/`](benchmark/per-tool/) (15 docs) | octocode CLI solo | every tool works, full schema | maintained — run before any comparison |
| [`octocode-vs-gh`](benchmark/compare/octocode-vs-gh/) | `gh` CLI vs octocode | GitHub research value vs standard CLI | **WIN 2026-08-02** (1.00 vs 0.67 uncontaminated; 0.098× bytes) |
| [`octocode-vs-gh-rtk`](benchmark/compare/octocode-vs-gh-rtk/) | `rtk`+`gh` vs octocode | value vs token-optimized baseline (ship-gate) | **scored 2026-08-02**: correctness TIE 0.90/0.90; 0.24× bytes; 1.4× wall-clock |
| [`octocode-vs-ast-grep`](benchmark/compare/octocode-vs-ast-grep/) | `ast-grep` vs octocode | AST parity + beyond-AST | re-authored on React corpus — needs scored run |
| [`octocode-vs-baseline-local-react`](benchmark/compare/octocode-vs-baseline-local-react/) | bare POSIX vs octocode | local research vs shell primitives | seeded @ 9ceb1e7d — needs scored run |
| [`octocode-mcp-vs-cli`](benchmark/compare/octocode-mcp-vs-cli/) | MCP vs CLI surface | context-token cost per surface + data parity | draft — verify oracles first |

Each compare suite = 10 questions. `questions.md` = solver-facing, frozen. `ground-truth.json` = judge-only.

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
