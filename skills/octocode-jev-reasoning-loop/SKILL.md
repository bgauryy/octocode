---
name: octocode-jev-reasoning-loop
description: "Use when semantic triage can avoid expensive reads, independent conditions share evidence, or an unresolved hypothesis, inference, or consequential plan needs judgment. Skip exact checks, unchanged votes, and calls that cannot change the next action."
---
# Octocode Jev reasoning loop

tools: `npx octocode` / `octocode-mcp`
related-skill: `octocode-research`
output: `<workspace>/.octocode/` for workspace work | `<home>/.octocode/` when no workspace applies
routes: load/run a reference, doc, script, or scheme only when it changes the next action; otherwise use the entry point below.

Jev returns typed probabilities over supplied evidence, not explanations or new facts. The host owns evidence, policy and action.

## Choose one useful call

| Unresolved work | Route | Next step |
|---|---|---|
| Mostly irrelevant candidates would require expensive reads | `jevScout`; standalone `scripts/code-scout.mjs` / `scripts/scout.mjs`; fetched history rows `scripts/pr-triage.mjs` | Inspect read and gray_read candidates; widen incomplete excerpts. |
| Independent semantic conditions over selected shared evidence | `scripts/ask-file.mjs --questions` or `--aspects`; structured input `scripts/profile.mjs` | Batch questions once; apply caller thresholds and AND/OR in code. Conditions use the profile path, not a separate native tool. |
| An unresolved alternative, inference, or consequential plan | `scripts/run-loop.mjs` | One bounded review, then its discriminating check or revision. The runner builds, validates and applies internally. |

Use exact reads, AST/LSP or tests when they settle the question. Skip scouting when every candidate must be read. Counts alone do not trigger calls. Do not chain scout → conditions → reasoning automatically, or add a final gate after evidence already settles the action.

## Evidence and questions

- Supply the decision, scope, anchored evidence and counterevidence; omit conversation history and private scratch reasoning.
- Questions are self-contained: IDs carry no instructions and answers are independent. Batch identical selected state. Dependent questions need new state or an explicit premise.
- Noul is P(yes); Choice selects unordered alternatives; Score uses ordered levels. Confidence is distribution concentration, not correctness. Thresholds are question/primitive-specific.
- Name the function and input. Instead of presupposing support with “does it preserve contentRef labels?”, use one Choice: preserves/replaces/unsupported/insufficient.
- Hypotheses need alternatives and a distinguishing check; classification does not. Execute an available cheap deciding test directly.
- Before asserting source behavior, inspect decisive original evidence if not already inspected and current. Reread for changed or incomplete evidence, not merely because Jev ran.
- Never prove absence from a skip, force uncertainty into a boolean, repeat unchanged votes, or treat Jev as authorization. Errors are not false conditions.

## Run

Use absolute skill paths from the workspace. When setting credentials, follow `references/configuration.md`. Keep inputs, responses, decisions and logs under `<output>/octocode-jev-reasoning-loop/`; scratch under `<output>/tmp/octocode-jev-reasoning-loop/`. Explicit runner outputs belong there; requested source edits retain their paths.

For independent source questions:
`node <skill-dir>/scripts/ask-file.mjs --files src/retry.ts --questions "Does it retry failed requests? || Does it implement backoff?" --model jev-1.13.0`

For a scoped typed profile, use `--aspects aspects.json --context "Public input accepted by parseInput"`; structured input is `assets/profile-input.schema.json`. For reasoning, use `scripts/run-loop.mjs --input compact.json` with `assets/run-loop-input.schema.json`. Dry-run unfamiliar input shapes; do not add a dry-run before every valid invocation.

Keep deciding spans complete. `contentRef` fails above its character budget (default 1200, maximum 4000); narrow to a complete span or raise the bound. Pin models for comparisons and retain model, usage, anchors, coverage and artifacts.

## Depth routes

- Candidate extraction and read policy → `references/scout.md`.
- Conditions, typed profiles and applicability → `references/profile.md`.
- Reasoning route and state → `references/routing.md`; packet debugging → `references/research.md`.
- Evidence selection → `references/context.md`; composed applications → `references/patterns.md`.
- API primitives/limits → `references/protocol.md`; primary sources → `references/references.md`.
- Workflow benefit and guardrails → `references/benchmark.md`.

After runtime changes, run `npm test` and the `octocode-skills` reviewer. Build and standalone packaging: `README.md`.
