---
name: octocode-eval-benchmark
description: "Use when designing evals, calibrating LLM judges, or measuring whether a change helped: baselines, held-out cases, overfitting controls, and keep/discard loops. Not for ordinary ship checks where tests passing is enough."
---
# Octocode eval benchmark
tools: `npx octocode` / `octocode-mcp`
related-skill: `octocode-research`
output: `<workspace>/.octocode/` for workspace work | `<home>/.octocode/` when no workspace applies
routes: load/run a reference, doc, or script only when it changes the next action; otherwise keep the rule here.

Separate real improvement from noise, leakage, and grader gaming.
```mermaid
flowchart LR
  F["FRAME"] --> H["VALIDATE HARNESS"] --> B["BASELINE"] --> D["DEVELOP"]
  D -- "KEEP / revert" --> D
  D -- "candidate selected" --> S["SEALED VERIFY"] --> X["DECIDE"] --> L["LEARN"]
  X -- "INVALID: correct and version harness, rerun both" --> H
  F -. "freeze goal, KPI, guardrails, budget, splits" .-> KC["kpi-contract.md"]
  F -. "build cases from failures; choose or retire a public benchmark" .-> EA["error-analysis.md"]
  F -. "subject is a multi-stage or multi-agent workflow" .-> MA["multi-agent.md"]
  H -. "build or extend a runner" .-> EH["eval-harness.md"]
  H -. "dispatch evaluated workers; audit leakage" .-> CL["clean-lab.md"]
  H -. "choose graders and metrics; grade tool or multi-turn process" .-> GR["graders.md"]
  H -. "a model or human must judge quality" .-> LJ["llm-judge.md"]
  D -. "improve the subject; pick the loop level" .-> AL["agent-loop.md"]
  S -. "select or accept a candidate; splits, uncertainty, verdicts" .-> HG["held-out-and-guards.md"]
  X -. "results fail, fluctuate, or improve suspiciously" .-> FR["failure-repair.md"]
  L -. "improve a skill, harness, or doc; write the report" .-> IL["improve-loop.md"]
```
Caption: develop on dev data; open the sealed test once, after selection; dotted edges load a page in `references/`.
Modes: **ErrorAnalyze** · **Define** · **Run** · **Suite** · **Benchmark** · **Audit**.
Pages (load each when its map edge fires): `references/kpi-contract.md` · `references/error-analysis.md` · `references/multi-agent.md` · `references/eval-harness.md` · `references/clean-lab.md` · `references/graders.md` · `references/llm-judge.md` · `references/agent-loop.md` · `references/held-out-and-guards.md` · `references/failure-repair.md` · `references/improve-loop.md`. Audit method provenance: `references/references.md`.

Definitions: `benchmarks/<name>/` (`benchmarks/README.md`); saved runs: `<output>/benchmarks/<name>/results/<run-id>/`. Keep evaluator artifacts outside solver access. Approved source edits keep their paths.

## Invariants
- Freeze the goal, primary KPI, meaningful effect threshold, guardrails, trial and selection budget, splits, and executable harness before you compare candidates. Version a corrected harness and rerun both sides.
- Each evaluated worker starts in a clean lab: only the production-equivalent task, subject instructions, and permitted inputs. Keep evaluator questions, answer keys, expected tool paths, prior attempts, and improvement feedback out of solver context and reachable storage.
- State legitimate task requirements; never hide requirements the grader enforces. Keep solution hints out. A deployment instruction under evaluation is part of the subject; an answer-specific coaching overlay is leakage.
- Use development feedback to iterate, validation to select, and a sealed final test to confirm. Repeated holdout feedback makes it development data. Record exposure and candidate count.
- Grade observable outcomes with deterministic checks where possible. Calibrate model judges against independent human labels for subjective quality. Before freezing, check positive, negative, ambiguous, and bypass examples.
- Select the candidate before you open final results; compare it with the baseline under the same conditions. Capture failures after the verdict; a suite or grader change starts a new version.
- Keep failures, Unknowns, grader errors, infrastructure errors, retries, and cost in the denominator and the report.
- Development KEEP is provisional. Final verdicts are ACCEPT, REVERT, INCONCLUSIVE (uncertain), or INVALID (compromised); the last two prove neither failure nor improvement.
- Public benchmarks orient; representative private tasks support product decisions. Public fixtures, regex checks, and self-tests check the grader, never generalization or agent behavior.
- `octocode-subagent` owns spawn mechanics; this skill owns how a multi-agent subject is measured.

Other owners: `octocode-research` proves code claims; `octocode-brainstorming` explores options; `octocode-prompt-optimizer` improves wording; `octocode-skills` reviews folders; `octocode-rfc-generator` decides consequential designs.

## Maintainer verification
After maintainer edits, run `node scripts/check-description.mjs` (metadata), `node scripts/eval-skill.mjs --self-test` (grader mechanics), then the `octocode-skills` review. `benchmarks/skill-smoke/README.md` documents case and batch checks.
