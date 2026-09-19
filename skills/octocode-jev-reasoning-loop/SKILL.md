---
name: octocode-jev-reasoning-loop
description: "Use when source candidates need semantic triage, several questions share the same context, or a bounded hypothesis, plan, or evidence claim needs a second judgment. Jev returns typed probabilities; the host owns research and action. Skip exact lookups, tests, and judgments that cannot change the next step."
---
# Octocode Jev reasoning loop

tools: `npx octocode` / `octocode-mcp`
related-skill: `octocode-research`
output: `<workspace>/.octocode/` for workspace work | `<home>/.octocode/` when no workspace applies
routes: load/run a reference, doc, script, or scheme only when it changes the next action; otherwise use the simplest entry point below.

Flow: `FRAME → SELECT CONTEXT → ASK → APPLY POLICY → VERIFY`

Save generated inputs, requests/responses, decisions, reports, and logs under `<output>/octocode-jev-reasoning-loop/`; scratch under `<output>/tmp/octocode-jev-reasoning-loop/`. Resolve `<output>` using the workspace/home `.octocode/` rule above, and point explicit runner output paths there. Keep run artifacts out of the installed skill folder. Chat-only results stay in chat; requested source edits keep their paths.

Jev evaluates supplied context and typed questions. It returns probabilities, not explanations or new evidence. Use it where semantic judgment helps; use Octocode search, AST, LSP, exact reads, and tests for facts and verification.

## Choose the useful call

| Need | Entry point | Host action |
|---|---|---|
| Prioritize expensive candidate reads | `jevScout` when available; otherwise `scripts/code-scout.mjs` or `scripts/scout.mjs` | Read accepted and uncertain (`gray_read`) candidates; widen retrieval when excerpts are incomplete. |
| Triage fetched PR or issue rows | `scripts/pr-triage.mjs` | Fetch the original item before drawing a conclusion. |
| Ask independent questions about files or fetched text | `scripts/ask-file.mjs --questions` for Noul; `--aspects` for typed questions | Supply scope with `--context`; retain unsupported/insufficient alternatives when a binary question would assume a feature exists. |
| Score dimensions or choose from explicit alternatives | `scripts/profile.mjs` with `assets/profile-input.schema.json` | Combine judgments in code and retain uncertainty. |
| Review a hypothesis, consequential plan, or bounded claim | `scripts/run-loop.mjs` with `assets/run-loop-input.schema.json` | Follow the provisional result with a source check, test, or revision. |

Scouting pays when it avoids substantial reads. Skip it for a known target or files you must read anyway. Batch questions only when their answers matter and deterministic checks do not settle them.

## Frame and supply context

State the decision, relevant facts with source anchors, and what a different answer would change. Each question must be self-contained: IDs are not instructions, and questions in one request cannot see each other's answers. Batch independent questions over shared state; a dependent question needs new state or an explicit speculative premise.

Use `noul` for P(yes), `choice` for competing options, and `score` for ordered rubric levels. Include an unknown/no-match outcome where needed. Supply decisive context and counterevidence, not the full conversation. Share concise observations and assumptions, never private scratch reasoning.

Name the function, input shape, and boundary that matter. “Does it preserve labels when resolving contentRef?” assumes contentRef is supported. Ask about acceptance first, or use one Choice with preserves/replaces/unsupported/insufficient outcomes. Keep counting, comparisons, cost tie-breaking, and dependent branches in code. Noul probabilities, Choice probabilities, and distribution confidence are different signals; do not transfer thresholds between them.

For hypotheses, supply alternatives and a distinguishing check. Source classification does not need an artificial hypothesis deck or falsifier. If a lookup, test, or already-clear action settles the question, take that step.

## Run and verify

From the workspace, use the runner's absolute skill path. Setup and explicit `.env` loading are in `references/configuration.md`.

```sh
node <skill-dir>/scripts/ask-file.mjs --files "src/retry.ts" --questions "Does it retry failed requests? || Does it implement backoff?"
node <skill-dir>/scripts/ask-file.mjs --files "src/input.ts" --aspects aspects.json --context "Judge the public input accepted by parseInput." --model jev-1.13.0
node <skill-dir>/scripts/run-loop.mjs --input compact.json --dry-run
node <skill-dir>/scripts/run-loop.mjs --input compact.json
```

Treat distributions as provisional judgments. Do not turn a close probability into certainty, infer absence from a skipped excerpt, or repeat a call to obtain approval. Retrieve missing evidence or narrow the question honestly. Reopen original sources before asserting behavior; Jev never replaces code evidence or grants permission to act.

Selected `contentRef` evidence now fails before the API call if it exceeds its character budget. Choose a complete deciding span or raise the limit up to 4000; do not remove counterevidence to get a passing judgment. Pin a model for comparisons and retain the emitted model, usage, anchors, coverage, and request/response artifacts.

## Depth routes

- Candidate retrieval, taxonomies, and read policy → `references/scout.md`.
- When defining reusable source questions → `references/profile.md`.
- When choosing reasoning routes and state → `references/routing.md`.
- When checking API primitives or limits → `references/protocol.md`; for primary sources → `references/references.md`.
- When bounding context or comparing sources → `references/context.md`; for composed workflows → `references/patterns.md`.
- When configuring credentials or network settings → `references/configuration.md`.
- When diagnosing runner failures → `references/research.md`.
- When measuring workflow benefit → `references/benchmark.md`.

After runtime changes, run `npm test` and the `octocode-skills` reviewer. Build and standalone packaging are documented in `README.md`.
