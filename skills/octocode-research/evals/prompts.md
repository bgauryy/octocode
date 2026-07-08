# Research Skill Smoke Evals

Use when changing `octocode-research`. Grade for evidence quality, route choice, and honest confidence. Run `scripts/eval-research.mjs --self-test` for deterministic output-shape checks, and use `evals/cases.json` for case definitions.

## Eval 1 — Code Investigation

Prompt: `Why does localSearchCode return an empty result for a symbol I can see in the file?`

Pass criteria: chooses Investigate; orients tree/path first; keeps two hypotheses; reads exact code; uses LSP/AST or search flags when relevant; explains whether the empty result is absence, indexing, path scope, or query-shape.

## Eval 2 — Prior-Art Map

Prompt: `Map current npm/GitHub options for structural TypeScript search.`

Pass criteria: chooses Map; searches packages and repositories; checks package health beyond downloads; fetches formal docs/papers when making method claims; reads source/README for top candidates; clusters solved/partial/abandoned/white-space.

## Eval 3 — OQL / Graph Proof

Prompt: `Find likely dead exports in this repo and say what is safe to delete.`

Pass criteria: reads `search --scheme --compact` before OQL JSON; treats research/graph rows as candidates; upgrades with exact reads, import/AST/LSP proof, and tests before deletion; gates broad removals.

## Eval 4 — Degraded Transport

Prompt: `Research why the CLI help mentions a flag that no longer works, but Octocode is unavailable.`

Pass criteria: falls back to `rg`/file reads/web, marks confidence degraded, avoids blocking on install unless required, and gives the exact install/auth next step only if needed.

## Eval 5 — Long Decision Brief

Prompt: `Prepare a decision brief on whether we should adopt a claim-level evidence ledger for long research tasks.`

Pass criteria: chooses Validate or Plan; recognizes the task is long/contested enough for `long-research.md`; freezes scope, surfaces, budget, and stop gates; tracks atomic claims separately from evidence; marks unsupported claims as gaps; keeps vendor research tools optional.

## Eval 6 — GitHub Landscape

Prompt: `Map GitHub repos that implement structural TypeScript search and recommend what we could reuse.`

Pass criteria: chooses Map or GitHub landscape; builds a repo DB with fit/activity/evidence/reuse/risk fields; searches repositories and packages; exact-reads top candidates; ranks stars/downloads only as tiebreakers; outputs clusters and an integration blueprint with proof anchors.

## Eval 7 — Change Mode

Prompt: `Refactor the formatDate utility in src/utils/date.ts to use Intl.DateTimeFormat instead of moment.js.`

Pass criteria: chooses Change mode; checks blast radius (LSP callers/references) before editing; reads the existing function exactly; makes the smallest scoped patch that follows from the evidence; reports the actual verification command that ran (build, test, or typecheck); does not claim success before verification runs.

## Eval 8 — PR / Local Review

Prompt: `Review my staged changes before I open a PR.`

Pass criteria: chooses Review; collects scope via `git status`/`git diff --staged` (or local tools); classifies risk per file (HIGH vs LOW) and sizes the pass (Quick vs Full) instead of always going deep; traces blast radius with LSP callers/references matching the change shape (not just a text search); checks domains in priority order (Security, Bug, Flow Impact, ...); each finding has severity, confidence, `file:line`, evidence, and a fix; avoids `#1`/`#2` finding labels; does not silently expand scope beyond staged changes.
