---
name: octocode-eval-benchmark
description: "Use when designing evals, calibrating LLM judges, or measuring whether a change helped: baselines, held-out cases, overfitting controls, and keep/discard loops. Not for ordinary ship checks where tests passing is enough."
---
# Octocode eval benchmark
tools: `npx octocode` / `octocode-mcp`
related-skill: `octocode-research`
output: `<workspace>/.octocode/` for workspace work | `<home>/.octocode/` when no workspace applies
routes: load/run a reference, doc, or script only when it changes the next action; otherwise keep the rule here.

Design evaluations that distinguish real improvement from noise, leakage, and grader gaming.
Flow: `FRAME → VALIDATE HARNESS → BASELINE → DEVELOP → SEALED VERIFY → DECIDE → LEARN`.
Modes: **ErrorAnalyze** · **Define** · **Run** · **Suite** · **Benchmark** · **Audit**.
Definitions: `benchmarks/<name>/`; saved runs: `<output>/benchmarks/<name>/results/<run-id>/`. Keep evaluator artifacts outside solver access. Approved source edits retain their paths.

## Invariants
- Freeze the goal, primary KPI, meaningful effect threshold, guardrails, trial/selection budget, splits, and executable harness before comparing candidates. Version a corrected harness and rerun both sides.
- Each evaluated worker starts in a clean lab: only the production-equivalent task, subject instructions, and permitted inputs. Keep evaluator questions, answer keys, expected tool paths, prior attempts, and improvement feedback out of solver context and reachable storage. Isolation includes files, memory, tools, and services, not just chat history.
- State legitimate task requirements; never hide requirements the grader enforces. Separate those requirements from solution hints. A deployment instruction being evaluated is part of the subject; an answer-specific coaching overlay is leakage.
- Use development feedback for iteration, validation for candidate selection, and a sealed final test for confirmation. Repeated holdout feedback makes it development data. Record exposure and candidate count.
- Grade observable outcomes with deterministic checks where possible; calibrate model judges against independent human labels for subjective quality. A fresh judge or a judge council is not automatically accurate.
- Keep exploratory KEEP separate from final ACCEPT. Uncertain evidence is INCONCLUSIVE; compromised trials are INVALID. Neither proves the candidate failed or improved.
- Public benchmarks orient; representative private tasks support product decisions. Public regex fixtures and self-tests check the grader, not generalization or agent behavior.

## Workflow and routes
1. **Frame:** use `references/error-analysis.md` for observed failures; `references/kpi-contract.md` connects the goal, measures and decision before an experiment.
2. **Validate harness:** use `benchmarks/README.md` when creating definitions or per-run results; `references/eval-harness.md` owns cases and run records; `references/clean-lab.md` owns solver/evaluator access. Choose graders with `references/eval-techniques.md`; use `references/llm-judge.md` when a model grades quality. Check positive, negative, ambiguous, and bypass examples before freezing.
3. **Baseline and develop:** run `references/agent-loop.md` on development data. Use `references/nested-loops.md` when deciding whether to change the subject, suite, or search strategy.
4. **Verify and decide:** `references/held-out-and-guards.md` owns sealed testing, uncertainty, and release verdicts. Select the candidate before opening final results; compare it with the baseline under the same conditions.
5. **Learn:** use `references/failure-repair.md` when diagnosing failures or suspicious gains; capture failures after the verdict; suite or grader changes start a new version. Use `references/improve-loop.md` for skill/harness changes and `references/output.md` for honest before/after reporting.

## Conditional routes
- Multi-agent subject: `references/subagent-cookbook.md` owns role separation, communication and outcome metrics. `references/graph-of-loops.md` owns dependency/attribution checks; `references/graph-failure-modes.md` owns shared-state and Goodhart risks. Use `octocode-subagent` only for authorized spawn mechanics.
- When grading tool or multi-turn tasks: `references/trajectory-grading.md` grades required constraints without prescribing an incidental solution path. Freeze live catalog/schemas; distinguish lexical `localSearch`, structural `astSearch`, exact `localFetch`, and semantic `lspSearch` evidence.
- When selecting a public suite: `references/benchmarking.md`. Method provenance and limitations: `references/references.md`.
- When another skill owns the next action: `octocode-research` proves code claims; `octocode-brainstorming` explores unresolved options; `octocode-prompt-optimizer` improves wording; `octocode-skills` reviews folders; `octocode-rfc-generator` handles consequential design decisions.

## Maintainer verification
After maintainer edits, use `scripts/check-description.mjs` for metadata and `scripts/eval-skill.mjs --self-test` for grader mechanics, then run the `octocode-skills` review. `benchmarks/skill-smoke/README.md` documents case/batch checks. Public fixtures check grader mechanics; use isolated behavioral trials before claiming performance gains. Report formatting is flexible.
