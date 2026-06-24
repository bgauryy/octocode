# Loop Protocol — Mechanics & Principles

Load for the reusable loop engine behind every mode: how to ground each step, keep state, escape stalls, and decide when to stop. Read the mode file (`research-loop.md`, `code-check-loop.md`, or `full-research-loop.md`) for the surface-specific recipe; read `tools.md` for transport and `status` details.

## Why the loop works

A closed loop — propose an action, run it against a tool that reports back, learn from the concrete outcome, repeat — consistently beats open-loop generation. The gain comes from **grounding**: the agent adapts to real feedback (a `status`, a match count, a measured result) instead of its prior. Removing feedback measurably degrades results; the loop is the point. Two corollaries drive every rule below:

1. **Delegate correctness to a tool that can prove it.** The agent explores and proposes; a ground-truth check (exact read, LSP, AST match, test run, history) decides whether a finding is real. Self-judging from a snippet produces confident-but-wrong conclusions — prove, don't assert. Trust signals in a strict order: **deterministic checks first** (test, build, type-check, AST/structural match, exact-string read), then an LLM-as-judge only for the genuinely unquantifiable, and the agent's own say-so last — never as the sole basis for a conclusion. Favour a deterministic check in every cycle that can carry one.
2. **History is your memory.** The running ledger of actions + observations is the episodic memory that lets the next iteration improve. Keep it compact and explicit, not implicit in scrollback.

## The iteration unit

Each iteration is one **action → grounded observation → learning**. Make the action the *cheapest call that could change the answer* — concise/discovery/path-only/symbols-only first, then exact reads, full files, PRs, or semantics only when the cheap pass justifies it. One call per iteration keeps the observation attributable.

Two-stage checking, cheap before expensive: filter with a fast pass (does the symbol/path/string even exist? a concise or path-only search), and only spend an expensive call (full-file read, clone + AST/LSP, test run) on candidates that survive. This mirrors lightweight-validate-then-formally-verify and avoids burning budget on dead leads.

## State ledger

Maintain a tiny ledger across iterations (in your working notes, not re-derived each turn):

- **Goal** — the framed question and the end condition.
- **Anchors** — paths, lines, match ranges, repo/PR/package ids, branches, `next.*` cursors. Carry these forward verbatim; never invent offsets or paths.
- **Hypotheses** — likely answer, alternate(s), and the observation that would disconfirm each.
- **Tried** — query shapes already run and their `status`, so you don't repeat a verbatim miss.

## Context compaction

A long loop fills the window with stale reasoning, raw dumps, and dead-end output — quality degrades silently. Between iterations, keep the ledger small: summarize what an observation *concluded* and drop its raw body, prune anchors that are no longer live, and offload bulky results to a note you can re-fetch by anchor rather than holding them inline. In a multi-source loop, run each surface as an **isolated sub-loop in a clean context** and pass only its distilled findings up — don't let one surface's scrollback crowd out the next.

## Learning from each status

- `empty` is data, not a dead end: the query ran and matched nothing. Adjust ONE variable (scope, spelling, branch, extraction mode, filter) and re-run, or switch surface. Only conclude absence after a deliberate broadening pass — a tighter follow-up (e.g. an exact-path read) can still confirm it.
- `error` carries a reason: fix the call (auth, validation, rate limit, scope) and retry corrected. Don't treat an `error` as "not found".
- results: extract anchors immediately; they are the inputs to the next action.

## Escaping stalls and local optima

Agents get stuck — repeating a query, re-proposing a rejected idea, or stopping after the first good hit. Counter:

- **Change a variable, not nothing.** If two iterations return the same `empty`/`error`, change the surface (local↔GitHub↔npm↔history) or the query *shape* (text↔structural↔semantic↔path), not just wording.
- **Push past premature stop.** If the framed question isn't fully answered, force one more disconfirming step before declaring done.
- **Best-of-K for hard targets.** When a loop converges weakly or the answer is high-stakes, restart the loop from a different angle (different entry surface, keywords, or hypothesis) and reconcile. Independent paths that agree raise confidence; disagreement exposes a gap.

## Failure modes to guard against

These are the ways a grounded loop still goes wrong — watch for each:

- **Reward hacking** — broadening or loosening a query until *something* matches, then treating the irrelevant hit as the answer. The match must satisfy the framed question, not just be non-empty.
- **Hallucinated success** — declaring "done / fixed / unused / always true" without the deterministic check that would falsify it. Run the check; report its actual outcome.
- **Compounding errors** — an early wrong anchor steers every later step. Verify early and often, not only at the end, so a bad lead dies cheap.
- **No-progress spin** — repeating a failing action. If the last N steps changed no state (same `empty`/`error`, same anchors), break and escalate instead of burning budget.

## Budgets and autonomy

Set caps up front, and treat them as real exits, not suggestions: an iteration cap (cheap question 3–5; normal 6–12; deep/multi-source scale per sub-loop), plus a token/wall-clock guard. The first 1–2 refinement rounds capture most of the reachable improvement — diminishing returns after that, so bias caps small and only extend when a round is still adding grounded evidence.

Keep the loop on a leash: spend cheap calls freely, but gate expensive or hard-to-reverse actions (clone, full-file reads, test runs, any write) on a surviving lead — and surface a human checkpoint before the genuinely irreversible ones. Raise autonomy gradually as the loop proves reliable on a task, rather than starting fully hands-off.

## Stop & reflect

Termination needs **multiple gates** — don't rely on any single one. Stop when ANY fires: the question is answered with grounded evidence and the alternate is killed; no cheap step can change the conclusion; the iteration or token budget is hit; or no-progress is detected. Before reporting: state the weakest claim, the strongest counter-evidence, and whether one more cheap call would flip it. If yes and budget remains, run it. Then emit a short loop trace: iterations run, the decisive observations (with anchors), and the final evidence.
