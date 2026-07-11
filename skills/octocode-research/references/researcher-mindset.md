# Researcher Mindset

The meta-layer: how to think, plan, measure, delegate, and stay efficient across a whole research task. `algorithm.md` owns the per-query route; this owns the campaign around it.

## State of Mind
- Hold the question above the tool. Prove or disprove a claim; search is only a means.
- Every result is a lead until a second, stronger grade agrees. One snippet, one grep, one LSP zero is never a conclusion.
- Keep two live hypotheses. Look for the cheapest step that KILLS one, not the step that flatters your favorite.
- Distrust silence: empty / zero / unsupported means "this lane can't see it," not "it isn't there." Say which one you mean.
- State confidence out loud — confirmed / likely / uncertain / weak — and what single check would change it.

## Plan & Manage the Work
- Open with one line: corpus, question, mode, active/skipped surfaces, budget, stop test.
- Keep a tiny ledger — `claim -> evidence -> confidence -> next check` — and compress large outputs into it before continuing.
- `loop-mode.md` owns the iteration budget, stop tests, and the ledger/anchor mechanics (`path:line`, matchRanges, ids, branch/ref, cursors, `next.*` — never invented); this section is only the campaign framing around them.
- Gate the expensive moves: ask before cloning/running code, broad repo scans, expensive external research, or a product/architecture decision.

## Understand the Environment First
Before trusting any surface, learn what is actually available:
- `context` — protocol + tool list; `auth status` — GitHub reach; `lsp-server status <file>` — whether semantics exist for this language.
- Gating: the MCP-server surface can gate local tools (`ENABLE_LOCAL`) and clone (`ENABLE_CLONE`); the `npx octocode` CLI enables both by default. A disabled surface is a skipped surface — declare it, degrade confidence, don't fake it.
- Read the corpus shape before concluding from it: monorepo vs flat, installed version vs default branch (`node_modules` is ground truth for what actually runs), language mix (decides LSP vs `minify:"symbols"` fallback).

## Measure the Research
Progress is claims resolved, not calls made. Each iteration ask:
- Did this change a confidence label or kill a hypothesis? If not, change surface or query shape — don't repeat the same call.
- Coverage: for a nontrivial claim, inspect at least two of structure, stream, and connections. Cross-check impact, unused, only, safe, or absent claims across code, tests, scripts, and configs.
- Done when grounded evidence answers the question and rejects the alternative, no cheap step can change the conclusion, or the budget is hit. Report remaining gaps without padding certainty.

## Take Several Directions (subagents)
When a question is broad, contested, or splits into independent probes, fan out parallel subagents instead of one serial chain:
- One direction per worker — e.g. local proof vs upstream history vs prior-art landscape; or the SAME claim down different lanes (lexical / structural / semantic) so disagreement is forced into the open.
- Give each a tight brief and a structured return: `claim, evidence (path:line / URL / id), verdict, confidence`.
- Merge by hunting for CONFLICT first. A disagreement between directions is the finding, not noise to average away.
- Validate before trusting a worker: re-check its load-bearing anchor yourself. A returned `path:line` or verdict is still a lead until you confirm it.
- Gate: ask before spawning many workers, or when the extra directions would blow the budget.

## Efficiency: jump smart, dot to dot
- Every result hands you the next call — follow `next.*`, matchRanges, and pagination cursors instead of re-deriving them.
- Route by the strongest handle you already hold (`algorithm.md`); skip the hops that handle makes redundant.
- Orient cheap before reading deep: tree / discovery / counts / symbols cost little and aim the expensive exact reads.
- Batch independent probes into one call (up to 5). Spend an extra angle on a *claim*; spend an extra query on a *lookup*.
- Materialize once when 3+ remote reads or AST/LSP/negative proof are coming — one bridge call turns remote code into local-grade evidence.

Validate: `node scripts/eval-research.mjs --case campaign-combination`.
