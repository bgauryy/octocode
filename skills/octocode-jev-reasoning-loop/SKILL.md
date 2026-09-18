---
name: octocode-jev-reasoning-loop
description: "Use when a high-cost or difficult-to-reverse plan needs viability/risk review, or when a semantic research fork has no cheap deterministic check: competing hypotheses, changed evidence, or a bounded claim. Think openly first; Jev returns a typed provisional judgment over caller-supplied state. It never supplies facts or the answer. Skip when a lookup, test, version, or obvious action decides."
---
# Octocode Jev reasoning loop

tools: `npx octocode` / `octocode-mcp`
related-skill: `octocode-research`
output: `<workspace>/.octocode/` for workspace work | `<home>/.octocode/` when no workspace applies
routes: load/run a reference, doc, script, or scheme only when it changes the next action; otherwise use the single runner below.

Flow: `THINK (System-2 CoT) → GATE → CALL Jev (System-1) → TYPED PROVISIONAL JUDGMENT → UPDATE / ABANDON / ASSERT`.

**Two systems.** You supply deliberate reasoning (System-2 CoT); Jev is a fast typed primitive (System-1) that scores your alternatives. It owns no facts or conclusions and never becomes evidence.

## Step 1 — THINK (mandatory, before any call)

Externalize a bounded, shareable summary — never private scratch:

1. **Observations** — evidence with source anchors.
2. **Alternatives** — 2–5 answers you name.
3. **Falsifier** — what would kill the lead.
4. **Anchor check** — confirmation-only search, one-observation basis, or “obvious” wording means a real fork.

If thinking settles the step, **stop — do not call Jev.**

## Step 2 — GATE

| The step is… | Route | Call Jev? |
|---|---|---|
| A lookup, test, version, or exact source read gives one right answer | `deterministic` | **No** — decide from evidence |
| A needed fact, source, or result is absent | `missing_fact` | **No** — retrieve, or report insufficiency |
| Executing or revising a supplied plan is high-cost, difficult to reverse, or a close judgment | `decision_review` | **Yes** — review viability, primary risk, and evidence need |
| Collected evidence leaves one bounded factual or causal claim disputed | `disputed_inference` | **Yes** — check status and decisive evidence basis |

Judge the object, not the wording: “the plan is ready” remains `decision_review`; factual claim status uses `disputed_inference`. Skip forced calls that cannot change action.

## Step 3 — CALL (one judgment at the fork)

- **Costly plan?** proposal + assumptions + risks → `decision_review`. “More evidence” returns to deterministic retrieval, not a claim check.
- **Am I anchored?** an attractive lead not yet falsified → `hunch_check` (weak signal) or `hypothesis_triage` (2–5 leads + a discriminating check). Keep the lead provisional.
- **Did new evidence move me?** one material observation after a frozen check → `reflection_delta` (update, abandon, or reframe). Send only material new evidence.
- **Does collected evidence settle one proposition?** one bounded factual or causal claim → `disputed_inference` (supported, contradicted, insufficient, or conflicting). If blocked, reopen the selected source; do not repeat-vote.
- **Safe to assert?** an evidence-backed claim about to be stated → `hallucination_gate` (proceed, qualify, or block).

Build compact input from `assets/run-loop-input.schema.json` (route + `willChangeAction` + route state), then:

```sh
node scripts/run-loop.mjs --input compact.json --dry-run   # validate the packet
node scripts/run-loop.mjs --input compact.json             # live judgment
```

The runner routes deterministically, skips inert and direct-check calls, validates the packet, evaluates Jev, and saves artifacts. Keep it 1–4k input tokens, shareable state only. One judgment per fork; a second needs material new evidence via `reflection_delta`.

## Conditional depth

- When routing or gating is disputed → `references/routing-policy.md`, then `references/routing.md`.
- When packet/API, configuration, context, or composition details affect execution → `references/protocol.md`, `references/configuration.md`, `references/context.md`, `references/patterns.md`.
- When debugging a runner failure → `references/research.md`; it owns `scripts/route-decision.mjs`, `scripts/build-decision-packet.mjs`, `scripts/validate-decision-packet.mjs`, `scripts/jev.mjs`, `scripts/research.mjs`, `scripts/check-research.mjs`, and `scripts/apply-response.mjs`. Normal use stays on `run-loop.mjs`.
- Before claiming benefit → `references/benchmark.md`. Primary KPI: **Wrong-Lean Recovery Rate** on disputed-inference cases with an attractive wrong lead; forcing Jev onto deterministic lookups proves overhead, not benefit — gate first.
- For live-call auth, set `OCTOCODE_JEV_KEY` per `references/configuration.md`.
- For provenance load `references/references.md`; for a structured host checkpoint load `references/deliberation.md`.

After runtime edits run `npm test`; it owns `scripts/test.mjs`, `scripts/research.test.mjs`, `scripts/decision.test.mjs`, `scripts/run-loop.test.mjs`, `scripts/verify-reasoning-loop.mjs`, `scripts/eval-decision-loop.mjs`, `scripts/eval-run-loop.mjs`, and `scripts/eval-recovery-heldout.mjs`. Then run the `octocode-skills` reviewer. Packaging uses `package.json`, `scripts/build.mjs`, and `bin/octocode-jev-darwin-arm64`. Artifacts go under `<output>/octocode-jev-reasoning-loop/`; scratch uses `<output>/tmp/octocode-jev-reasoning-loop/`.
