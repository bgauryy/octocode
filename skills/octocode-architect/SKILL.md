---
name: octocode-architect
description: "Use when an architecture decision or refactor needs evidence about boundaries, contracts, data/control flow, coupling, blast radius, cycles, reachability, performance, or maintainability. Not for evidence collection without a decision → octocode-research; behavior-preserving cleanup → octocode-clean-agentic-code."
---

# Octocode Architect

tools: `npx octocode` / `octocode-mcp`
related-skill: `octocode-research`
output: `<workspace>/.octocode/` for workspace work | `<home>/.octocode/` when no workspace applies
routes: load/run a reference, doc, or script only when it changes the next action; otherwise keep the rule here.

Model the system, test architecture hypotheses against code and runtime evidence, then make the smallest authorized, verified improvement.

Flow: `FRAME → MODEL → PROVE → CHANGE → VERIFY`.

## Rules

- Before editing, name the use case, quality attribute, contract owner, and smallest useful slice. An unresolved use case, owner, or contract is a decision gap—not permission to guess.
- Separate declared architecture, observed structure, and inferred intent. A graph edge, code smell, folder name, or pattern preference is a hypothesis, not a flaw.
- Trace static dependencies, runtime control, external/internal data movement, contract behavior, and invariant ownership separately; one lane cannot prove another.
- For repo, GitHub, package, or symbol evidence, use `octocode-research`; it owns tool selection and live schemas. This skill owns architecture analysis.
- Before editing, map interfaces, consumers, stored data, operations, tests, rollout, and rollback; retrace them in the diff.
- Improvement needs a sensor: baseline → change one variable → rerun comparably → keep only demonstrated improvement.
- Refactor when the request allows edits and evidence shows a smaller, clearer seam improves the named quality. Review-only requests return findings and a plan, not edits.
- Preserve concurrent work. Inspect the working tree, coordinate overlapping paths, and never stash, reset, overwrite, or discard another contributor's changes.
- Scale rigor to consequence. Do not invent layers, abstractions, findings, or operational work to satisfy a checklist.

## Workflow

1. **FRAME** — define the decision, constraints, quality attribute, scope, entrypoints, tests policy, budget, and consequence being predicted.
2. **MODEL** — load `references/architecture-lenses.md` when boundaries, layers, interfaces, flows, ownership, or blast radius matter; trace 1–3 representative scenarios. Load `references/contract-data-flow-checks.md` when a contract or external/internal data path crosses a trust, process, package, persistence, or ownership boundary.
3. **PROVE** — load `references/algorithm-review.md` when algorithms or data structures can fail on correctness, bounds, termination, complexity, concurrency, or numerical behavior. Load `references/architecture-analysis.md` when dependency topology, cycles, reachability, dead code, duplication, coupling, separation, hot paths, or efficiency can change the decision.
4. **CHANGE** — load `references/refactoring.md` when a proven issue needs a safer boundary or migration seam. Before any source edit, load `references/delivery-discipline.md`; freeze the owned behavior, implement one vertical slice, and exercise the production path.
5. **VERIFY** — inspect the diff and rerun comparable focused checks. For agent-authored work, load `references/agent-defect-classes.md`; load `references/agent-defect-evidence.md` only when prioritizing or disputing a defect class. Use `references/output-contracts.md` when a consequential plan or review needs an auditable result.

## Gates

- Report a candidate—not a defect—when decisive scope, flow, symbol identity, runtime impact, or measurement remains unresolved.
- Ask before public-contract rewires, cross-package moves, schema/storage migrations, or deletes/renames unless the user already authorized that exact scope.
- Cleanup stays caused by or directly adjacent to the change. Broader refactoring requires evidence, an acceptance sensor, and authorization. <!-- style-lint: ignore-line passive-voice -->
- Bookkeeping follows the repo and release contract: update only required docs, manifests, versions, generated artifacts, lockfiles, changelogs, or snapshots.
- Unimplemented reachable paths fail explicitly. Tests prove outcomes, not merely calls.
- Chat-only output stays in chat. Requested source edits stay in their named repo; do not create planning artifacts unless asked.

For skill maintenance, run `node scripts/eval-architect.mjs --self-test`, `node scripts/eval-architect.mjs --json`, and the `octocode-skills` review. Research provenance is in `references/references.md`; load it only when auditing these rules.
