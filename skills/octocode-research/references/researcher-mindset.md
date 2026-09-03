# Researcher Mindset

Load when a research task needs a campaign, not a query: planning, budgets, measuring progress, delegating, or staying efficient across the whole run. Why: `references/algorithm.md` owns the per-query route; this owns the meta-layer around it.

## Campaign stance
- Prove or disprove the claim; tools are means, and every result remains a lead until stronger evidence agrees.
- Keep two hypotheses and choose the cheapest check that can eliminate one.
- `empty`, `zero`, or `unsupported` means the named lane cannot see it, not universal absence.
- State `confirmed`, `likely`, `uncertain`, or `weak`, plus the check that can change it.

## Campaign control
- Open with one line: corpus, question, mode, active/skipped surfaces, budget, stop test.
- Keep a tiny ledger — `claim -> evidence -> confidence -> next check` — and compress large outputs into it before continuing.
- `references/loop-mode.md` owns the iteration budget, stop tests, and the ledger/anchor mechanics (`path:line`, matchRanges, ids, branch/ref, cursors, `next.*` — never invented); this section is only the campaign framing around them.
- Gate the expensive moves: ask before cloning/running code, broad repository scans, expensive external research, or a product/architecture decision.

## Environment
Before trusting a surface, learn what is available:
- `context` — protocol + tool list; `auth status` — GitHub reach; `lsp-server status <file>` — whether semantics exist for this language.
- Gating: CLI and MCP enable local tools (`ENABLE_LOCAL`) and clone (`ENABLE_CLONE`) by default; either can be explicitly disabled. A disabled surface is a skipped surface — declare it, degrade confidence, don't fake it.
- Read the corpus shape before concluding from it: monorepo vs flat, installed version vs default branch (`node_modules` is ground truth for what runs), language mix (decides LSP vs `minify:"symbols"` fallback).

## Progress
Measure claims resolved, not calls made:
- Did this change a confidence label or kill a hypothesis? If not, change surface, or query shape — don't repeat the same call.
- Coverage: for a nontrivial claim, inspect at least two of structure, stream, and connections. Cross-check impact, unused, only, safe, or absent claims across code, tests, scripts, and configs.
- Done when grounded evidence answers the question and rejects the alternative, no cheap step can change the conclusion, or the budget is hit. Report remaining gaps without padding certainty.

## Independent directions
For a broad, contested question with independent probes, parallel workers can reduce latency:
- One direction per worker — for example local proof vs upstream history vs prior-art landscape; or the SAME claim down different lanes (lexical / structural / semantic) so disagreement is forced into the open. <!-- style-lint: ignore-line passive-voice -->
- Give each a tight brief and a structured return: `claim, evidence (path:line / URL / id), verdict, confidence`.
- Reconcile conflicts first; disagreement is evidence to investigate, not noise to average.
- Validate before trusting a worker: re-check its load-bearing anchor yourself. A returned `path:line` or verdict is still a lead until you confirm it.
- Ask before enough workers to materially expand budget.

## Efficiency
- Every result hands you the next call — follow `next.*`, matchRanges, and pagination cursors instead of re-deriving them.
- Route by the strongest handle you already hold (`references/algorithm.md`); skip the hops that handle makes redundant.
- Orient cheap before reading deep: tree / discovery / counts / symbols cost little and aim the expensive exact reads.
- Batch independent probes into one call (up to 5). Spend an extra angle on a *claim*; spend an extra query on a *lookup*.
- Materialize once when 3+ remote reads or AST/LSP/negative proof are coming — one bridge call turns remote code into local-grade evidence.

Next: run the iterations under `references/loop-mode.md`; pick the route the campaign needs from `references/workflows.md`; when the campaign must produce a durable brief load `references/long-research.md`.
