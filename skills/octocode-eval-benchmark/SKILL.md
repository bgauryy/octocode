---
name: octocode-eval-benchmark
description: "Use when designing evals, calibrating LLM judges, or measuring whether a change helped: baselines, held-out cases, overfitting controls, and keep/discard loops. Not for ordinary ship checks where tests passing is enough."
---
# Octocode eval benchmark
tools: `npx octocode` / `octocode-mcp`
related-skill: `octocode-research`
output: `<workspace>/.octocode/` for workspace work | `<home>/.octocode/` when no workspace applies

Separate real improvement from noise, leakage, and grader gaming.
```mermaid
flowchart LR
  F["FRAME"] --> H["VALIDATE HARNESS"] --> B["BASELINE"] --> D["DEVELOP"]
  D -- "KEEP / revert" --> D
  D -- "candidate selected" --> S["SEALED VERIFY"] --> X["DECIDE"] --> L["LEARN"]
  X -- "INVALID: correct and version harness, rerun both" --> H
  F -. "before freezing goal, KPI, guardrails, budget, splits" .-> KC["references/kpi-contract.md"]
  F -. "when building cases from failures, or choosing or retiring a public benchmark" .-> EA["references/error-analysis.md"]
  F -. "when the subject is a multi-agent workflow" .-> MA["references/multi-agent.md"]
  H -. "when building or extending a runner" .-> EH["references/eval-harness.md"]
  H -. "before dispatching workers or auditing leakage" .-> CL["references/clean-lab.md"]
  H -. "when choosing graders, metrics, or process checks" .-> GR["references/graders.md"]
  H -. "when a model or human judges quality" .-> LJ["references/llm-judge.md"]
  D -. "when improving the subject or picking a loop level" .-> AL["references/agent-loop.md"]
  S -. "before selecting or accepting a candidate" .-> HG["references/held-out-and-guards.md"]
  X -. "when results fail, fluctuate, or improve suspiciously" .-> FR["references/failure-repair.md"]
  L -. "when improving a skill, harness, or doc, or reporting" .-> IL["references/improve-loop.md"]
```
Caption: develop on dev data; open the sealed test once, after selection; a dotted edge loads its page.
Modes: **ErrorAnalyze** · **Define** · **Run** · **Suite** · **Benchmark** · **Audit**.
Read `references/references.md` when you audit method provenance.

Definitions: `benchmarks/<name>/` (`benchmarks/README.md`); saved runs: `<output>/benchmarks/<name>/results/<run-id>/`. Keep evaluator artifacts outside solver access. Approved source edits keep their paths.

## Invariants
- Freeze the goal, primary KPI, meaningful effect threshold, guardrails, trial and selection budget, splits, and executable harness before comparing candidates. Version a corrected harness; rerun both sides.
- Each evaluated worker starts in a clean lab with only the production-equivalent task, subject instructions, and permitted inputs. Evaluator questions, answer keys, expected tool paths, prior attempts, and improvement feedback stay out of solver context and reachable storage.
- State every requirement the grader enforces; keep solution hints out. A deployment instruction under evaluation is subject; an answer-specific coaching overlay is leakage.
- Development iterates, validation selects, a sealed final test confirms. Repeated holdout feedback makes it development data. Record exposure and candidate count.
- Grade observable outcomes deterministically where possible; calibrate model judges against independent human labels. Before freezing, check positive, negative, ambiguous, and bypass examples.
- Select the candidate before opening final results; compare it with the baseline under the same conditions. Capture failures after the verdict; a suite or grader change starts a new version.
- Keep failures, Unknowns, grader errors, infrastructure errors, retries, and cost in the denominator and the report.
- Development KEEP is provisional. Final verdicts are ACCEPT, REVERT, INCONCLUSIVE (uncertain), or INVALID (compromised); the last two prove neither failure nor improvement.
- Public benchmarks orient; representative private tasks support product decisions. Public fixtures, regex checks, and self-tests check the grader, never generalization or agent behavior.
- `octocode-subagent` owns spawn mechanics; this skill owns how a multi-agent subject is measured.

Other owners: `octocode-research` proves code claims; `octocode-brainstorming` explores options; `octocode-agentic-prompts` improves wording; `octocode-skills` reviews folders; `octocode-rfc-generator` decides consequential designs.

## Maintainer verification
After maintainer edits, run `node scripts/check-description.mjs` (metadata), `node scripts/eval-skill.mjs --self-test` (grader mechanics), then the `octocode-skills` review. `benchmarks/skill-smoke/README.md` documents case and batch checks.
