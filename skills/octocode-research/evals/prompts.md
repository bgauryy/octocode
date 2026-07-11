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

## Eval 7b — Refactor Mode

Prompt: `Refactor packages/app/src/utils into packages/app/src/lib/utils — move the tree, keep public exports stable, and update all imports.`

Pass criteria: chooses Refactor mode; maps skeleton/structure before body work; freezes contracts/invariants (public exports); checks blast radius (LSP + lexical path hits); plans bulk `mv` plus mechanical path rewrites rather than copy-rewrite; reports verification that actually ran; does not blind-sed symbol identifiers.

## Eval 8 — PR / Local Review

Prompt: `Review my staged changes before I open a PR.`

Pass criteria: chooses Review; collects scope via `git status`/`git diff --staged` (or local tools); classifies risk per file (HIGH vs LOW) and sizes the pass (Quick vs Full) instead of always going deep; traces blast radius with LSP callers/references matching the change shape (not just a text search); checks domains in priority order (Security, Bug, Flow Impact, ...); each finding has severity, confidence, `file:line`, evidence, and a fix; avoids `#1`/`#2` finding labels; does not silently expand scope beyond staged changes.

## Eval 9 — Campaign / Combination

Prompt: `Our retry helper may have diverged from the upstream library we vendored it from. Plan and run the research to decide if we can delete the local copy — coordinate multiple directions.`

Pass criteria: checks the environment (auth/LSP/gating) before trusting surfaces; fans out parallel subagent directions (or lanes) for the broad question instead of one serial chain; uses the local↔external combination bridge (materialize upstream, then AST/LSP local-grade); measures progress by claims resolved with an explicit stop test; re-verifies each worker's key anchor and treats cross-direction disagreement as the finding; ends with confidence, `file:line` anchors, and one next action.

## Eval 10 — Local Research

Prompt: `Where is formatDate defined in this checkout, who calls it, and is the node_modules copy of date-fns the version that actually runs?`

Pass criteria: chooses Investigate; runs the local spine (tree/find → search → exact → LSP/structural); inspects `node_modules` before GitHub for package behavior; diffs lexical hits vs LSP for impact; cites `file:line`; ends with confidence and next.

## Eval 11 — External Research

Prompt: `How does vitejs/vite wire createServer in the published repo, and what recent PR touched the CLI entry?`

Pass criteria: chooses Investigate; follows discovery → structure → code/exact → history; treats GitHub search zeros as provider evidence (verify/materialize, not absence); cites `resolvedBranch`/ref; ends with confidence and next.

## Eval 12 — Loop Mode

Prompt: `Keep going until you can confirm whether export helpers is dead code — evidence keeps flipping between text hits and LSP.`

Pass criteria: chooses Loop; shows Act→Observe→Learn with cheap discovery before exact/LSP; distinguishes empty vs error; keeps a ledger/hypotheses and an explicit stop test; reports Loop trace of decisive iterations (not a full transcript); ends with answer, evidence, gaps/next.

## Eval 13 — Ambiguous Problem Classification

Prompt: `Fix this bug: the export API rejects a new output format requested by one customer.`

Pass criteria: does not trust the bug label; captures actual/desired/authority; keeps classification unknown until the supported contract is checked; distinguishes an unpromised capability as a feature; names one cheap decisive check; does not invent a root cause.

## Eval 14 — Feature Framing

Prompt: `Add streaming JSON output to the CLI while keeping current output stable.`

Pass criteria: classifies Feature; names the capability gap, consumers, compatibility decision, smallest contract-owning boundary, and observable acceptance tests; does not manufacture a defect mechanism.

## Eval 15 — Enhancement Framing

Prompt: `Enhance local code search so it feels much faster without changing results.`

Pass criteria: classifies Enhancement; converts “faster” into a measured baseline and target; identifies a bottleneck hypothesis and experiment; preserves result parity with a regression guard.
