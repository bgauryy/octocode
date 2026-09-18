---
name: octocode-jev-reasoning-loop
description: "Use when meaningful semantic uncertainty can change a research or software action: competing explanations, a costly check, contradictory evidence, a disputed inference, or a claim nearing assertion. Jev judges bounded host-supplied alternatives; it never supplies facts or solutions. Skip when a lookup, test, policy rule, or obvious next action already decides the step."
---
# Octocode Jev reasoning loop

tools: `npx octocode` / `octocode-mcp`
related-skill: `octocode-research`
output: `<workspace>/.octocode/` for workspace work | `<home>/.octocode/` when no workspace applies
routes: load/run a reference, doc, script, or scheme only when it changes the next action; otherwise use the single runner below.

Jev is a decision-discipline tool, not a solution generator. The host owns alternatives, facts, checks, policy, actions, and conclusions. Jev only scores bounded supplied choices; its output stays provisional and never becomes evidence.

Flow: `HOST REASONING → BOUNDED ALTERNATIVES → JEV TYPED JUDGMENT → PRECOMMIT PREDICTION → DISCRIMINATING CHECK → REAL EVIDENCE → JEV DELTA JUDGMENT → UPDATE / ABANDON / REFRAME → CLAIM CHECK → EVIDENCE GATE`.

## One entry point

Create compact input from `assets/run-loop-input.schema.json`, then run:

```sh
node scripts/run-loop.mjs --input compact.json --dry-run
node scripts/run-loop.mjs --input compact.json
```

The runner deterministically routes first, skips inert/direct-check calls, derives the required bounded reasoning summary, validates the route packet, evaluates Jev, checks disputed-claim consistency, creates provisional APPLY actions from host-supplied state, and saves artifacts. This summary carries auditable context, not hidden chain-of-thought. Use `--response recorded.json` for offline replay and `--output DIR` to select the artifact directory. Run `--help` for bounded network flags.

One uncertainty point normally gets one judgment. A second call requires material new evidence and uses `reflection_delta`. Do not force the full loop onto routine work.

## Host loop

1. State the uncertainty and whether a Jev answer can change the next action. Supply `directCheck` when code can decide it.
2. Supply a small credible deck; Choice decks include `none`. Jev must not invent alternatives.
3. For testable hypotheses, freeze each prediction, weakening condition, and check outcome before observing reality. Omit prediction/falsifier fields for classification and claim status when they add no information.
4. Execute the selected check yourself. Record source, scope, freshness, and the actual result.
5. Send only material new evidence through `reflection_delta`; then update, abandon, reframe, or replace the deck.
6. Check one bounded inference before assertion. If claim status and basis disagree, accept the runner’s observed-facts narrowing or retrieve new evidence—never repeat-vote unchanged state.
7. Gate the final scoped claim. Reopen the selected source before citing it.

## Route by signal

| Signal | Route |
|---|---|
| Weak signal; no deck | `hunch_check` |
| 2–5 testable explanations and branchable checks | `hypothesis_triage` |
| High-cost, close, or hard-to-reverse action | `decision_review` |
| New evidence after a frozen check | `reflection_delta` |
| Evidence for one bounded inference | `disputed_inference` |
| Evidence-backed claim nearing assertion | `hallucination_gate` |

Exact lookup, arithmetic, authorization, stale/missing facts, empty evidence, and scope mismatch stay deterministic. Send only task-relevant shareable state; never secrets, untrusted commands, or private deliberation.

## Conditional depth

- When routing or policy is disputed, load `references/routing-policy.md`, then `references/routing.md`.
- When packet/API, configuration, context, or composition details affect execution, load `references/protocol.md`, `references/configuration.md`, `references/context.md`, or `references/patterns.md` respectively.
- When debugging a runner failure, load `references/research.md`; it owns `scripts/route-decision.mjs`, `scripts/build-decision-packet.mjs`, `scripts/validate-decision-packet.mjs`, `scripts/jev.mjs`, `scripts/research.mjs`, `scripts/check-research.mjs`, and `scripts/apply-response.mjs`. Normal use stays on `scripts/run-loop.mjs`.
- Before claiming benefit, load `references/benchmark.md` and run its frozen benchmark. Primary semantic KPI: **Wrong-Lean Recovery Rate**. Passing contracts does not prove held-out benefit.
- For provenance, load `references/references.md`; for a structured host checkpoint, load `references/deliberation.md`.

After runtime edits run `npm test`, `scripts/eval-run-loop.mjs`, and `scripts/verify-reasoning-loop.mjs`; `npm test` owns `scripts/test.mjs`, `scripts/research.test.mjs`, `scripts/decision.test.mjs`, and `scripts/run-loop.test.mjs`. Run `scripts/eval-decision-loop.mjs` for routing and `scripts/eval-recovery-heldout.mjs` for recovery. Packaging uses `package.json`, `scripts/build.mjs`, and `bin/octocode-jev-darwin-arm64`. Then run the `octocode-skills` reviewer. Artifacts belong under `<output>/octocode-jev-reasoning-loop/`; scratch belongs under `<output>/tmp/octocode-jev-reasoning-loop/`.
