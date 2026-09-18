---
name: octocode-jev-reasoning-loop
description: "Use when you hit a research fork you cannot cheaply settle: an attractive-but-unproven lead, contradictory evidence, a hunch on thin basis, or a claim about to be asserted. Think in the open first (structured CoT), then let Jev score your named alternatives and return a typed, provisional judgment. Jev is a fast decision primitive that calibrates your reasoning; it never supplies facts, alternatives, or the answer. Skip when a lookup, test, version, or obvious next action already decides the step."
---
# Octocode Jev reasoning loop

tools: `npx octocode` / `octocode-mcp`
related-skill: `octocode-research`
output: `<workspace>/.octocode/` for workspace work | `<home>/.octocode/` when no workspace applies
routes: load/run a reference, doc, script, or scheme only when it changes the next action; otherwise use the single runner below.

Flow: `THINK (System-2 CoT) → GATE → CALL Jev (System-1) → TYPED PROVISIONAL JUDGMENT → UPDATE / ABANDON / ASSERT`.

**Two systems.** You supply slow, deliberate reasoning (System-2 CoT); Jev is a fast typed decision primitive (System-1) that scores the alternatives you named. It owns no facts or conclusions and never becomes evidence. The split fits any model size: the primitive runs on a small local model, and the THINK step gives a small host model the deliberation a large one has natively.

## Step 1 — THINK (mandatory, before any call)

Externalize a bounded, shareable summary — never private scratch:

1. **Observations** — what the evidence shows, with source anchors.
2. **Alternatives** — 2–5 competing answers you name (Jev never invents them).
3. **Falsifier** — the one observation that would kill your leading alternative.
4. **Anchor check** — have you only sought confirming evidence? Is your basis one observation? About to write "clearly/obviously"? Any "yes" is a real fork.

If thinking settles the step, **stop — do not call Jev.**

## Step 2 — GATE

| The step is… | Route | Call Jev? |
|---|---|---|
| A lookup, test, version, or exact source read gives one right answer | `deterministic` | **No** — decide from evidence |
| A needed fact, source, or result is absent | `missing_fact` | **No** — retrieve, or report insufficiency |
| Two source-backed interpretations remain, no cheap check settles them, and the choice changes your next action | `disputed_inference` | **Yes** — one bounded call |

A forced call on a decided question adds tokens and narrative without changing the answer. Most steps end here, uncalled.

## Step 3 — CALL (one judgment at the fork)

- **Am I anchored?** an attractive lead not yet falsified → `hunch_check` (weak signal) or `hypothesis_triage` (2–5 leads + a discriminating check). Keep the lead provisional.
- **Did new evidence move me?** one material observation after a frozen check → `reflection_delta` (update, abandon, or reframe). Send only material new evidence.
- **Safe to assert?** a claim about to be stated → `hallucination_gate` / `disputed_inference` (proceed, qualify, or block). If blocked, reopen the source.

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

After runtime edits run `npm test`, `scripts/eval-run-loop.mjs`, and `scripts/verify-reasoning-loop.mjs`; `npm test` owns `scripts/test.mjs`, `scripts/research.test.mjs`, `scripts/decision.test.mjs`, and `scripts/run-loop.test.mjs`. Run `scripts/eval-decision-loop.mjs` for routing and `scripts/eval-recovery-heldout.mjs` for recovery. Packaging uses `package.json`, `scripts/build.mjs`, and `bin/octocode-jev-darwin-arm64`. Then run the `octocode-skills` reviewer. Artifacts under `<output>/octocode-jev-reasoning-loop/`; scratch under `<output>/tmp/octocode-jev-reasoning-loop/`.
