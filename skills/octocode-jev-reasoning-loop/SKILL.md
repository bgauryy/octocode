---
name: octocode-jev-reasoning-loop
description: "Use when a high-cost or difficult-to-reverse plan needs viability/risk review, or when a semantic research fork has no cheap deterministic check: competing hypotheses, changed evidence, or a bounded claim. Think openly first; Jev returns a typed provisional judgment over caller-supplied state. It never supplies facts or the answer. Skip when a lookup, test, version, or obvious action decides."
---
# Octocode Jev reasoning loop

tools: `npx octocode` / `octocode-mcp`
related-skill: `octocode-research`
output: `<workspace>/.octocode/` for workspace work | `<home>/.octocode/` when no workspace applies
routes: load/run a reference, doc, or script only when it changes the next action; otherwise use the single runner below.

Flow: `THINK → GATE → CALL Jev → TYPED PROVISIONAL JUDGMENT → UPDATE / ABANDON / ASSERT` (ASSERT only through `hallucination_gate`). Jev scores caller-supplied alternatives; it owns no facts and never becomes evidence.

## THINK (before any call)

Externalize shareable state — never private scratch: observations with source anchors; 2–5 named alternatives; a falsifier; an anchor check (confirmation-only search or one-observation basis means a real fork). If thinking settles the step, stop — do not call Jev.

## GATE, then route

| Step | Route | Jev? |
|---|---|---|
| A lookup, test, or exact read gives one answer | `deterministic` | No — decide from evidence |
| A needed fact or source is absent | `missing_fact` | No — retrieve or report insufficiency |
| A supplied plan is high-cost, hard to reverse, or a close call | `decision_review` | Yes — viability, primary risk, evidence need |
| An unfalsified attractive lead | `hunch_check`; 2–5 leads + a discriminating check → `hypothesis_triage` | Yes — keep the lead provisional |
| One material observation after a frozen check | `reflection_delta` | Yes — update, abandon, or reframe |
| Evidence leaves one bounded claim disputed | `disputed_inference` | Yes — status + decisive basis; if blocked, reopen the source, never repeat-vote |
| An evidence-backed claim about to be stated | `hallucination_gate` | Yes — proceed, qualify, or block |

Judge the object, not the wording: "the plan is ready" is `decision_review`; claim status is `disputed_inference`. Skip calls that cannot change action.

## CALL

Build compact input from `assets/run-loop-input.schema.json` (route + `willChangeAction` + route state):

```sh
node scripts/run-loop.mjs --input compact.json --dry-run   # validate the packet
node scripts/run-loop.mjs --input compact.json             # live judgment
```

The runner gates, validates, evaluates, and saves artifacts. Keep packets 1–4k tokens. One judgment per fork; a second needs material new evidence via `reflection_delta`.

## Depth routes

- When triaging PR/issue search rows end-to-end → run `scripts/pr-triage.mjs` (gh rows → one scout → single fetch). When triaging candidate code files for "which one implements X" end-to-end → run `scripts/code-scout.mjs` (paths + question → one batched scout with an `is_code` docs veto → read only the implementer; rejected files' bytes stay off host). When many candidate files or rows need triage → run `scripts/scout.mjs` per `references/scout.md`; when profiling selected sources → run `scripts/profile.mjs` per `references/profile.md`.
- When routing or gating is disputed → `references/routing-policy.md`, then `references/routing.md`.
- When packet/API, configuration, context, or composition details affect execution → `references/protocol.md`, `references/configuration.md`, `references/context.md`, `references/patterns.md`.
- When debugging a runner failure → `references/research.md`, which owns `scripts/route-decision.mjs`, `scripts/build-decision-packet.mjs`, `scripts/validate-decision-packet.mjs`, `scripts/jev.mjs`, `scripts/research.mjs`, `scripts/check-research.mjs`, and `scripts/apply-response.mjs`; normal use stays on `scripts/run-loop.mjs`.
- When claiming benefit → `references/benchmark.md`; KPI: Wrong-Lean Recovery Rate — gate first.
- When configuring live-call auth (`OCTOCODE_JEV_KEY`) → `references/configuration.md`; when citing provenance → `references/references.md`; when writing a host checkpoint → `references/deliberation.md`.

After runtime edits run `npm test`; it runs `scripts/test.mjs`, `scripts/research.test.mjs`, `scripts/decision.test.mjs`, `scripts/run-loop.test.mjs`, `scripts/resolve-content-ref.test.mjs`, `scripts/scout.test.mjs`, `scripts/profile.test.mjs`, `scripts/verify-reasoning-loop.mjs`, `scripts/eval-decision-loop.mjs`, `scripts/eval-run-loop.mjs`, `scripts/eval-recovery-heldout.mjs`, and `scripts/eval-content-ref.mjs`. Then run the `octocode-skills` reviewer. Packaging: `package.json`, `scripts/build.mjs`, `bin/octocode-jev-darwin-arm64`. Artifacts: `<output>/octocode-jev-reasoning-loop/`; scratch: `<output>/tmp/octocode-jev-reasoning-loop/`.
