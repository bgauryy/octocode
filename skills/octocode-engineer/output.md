# Output

<!-- report-contract {"statuses":["Proposed","Accepted","Needs work"],"required":["Findings","Decision","Checks"],"anyOf":[]} -->

Answer in chat unless the [saved-report rule](#saved-report) applies or the user asks to save. Task content only: no probe output or process metadata. Keep assumptions and uncertainty next to the claims they affect.

## Response

BLUF: the decision or finding and its confidence first, then the boundary or component it affects, evidence, trade-offs, limits and the next check.

## Design answer

1. **Recommendation.** The choice in one sentence and the workload assumption it rests on.
2. **Why.** The deciding trade-offs with units, each number labelled measured, documented or estimated.
3. **Alternatives.** The strongest rival and the condition under which it wins.
4. **Failure modes.** What breaks, how it is detected and the mitigation.
5. **Check.** The benchmark, test, query plan or tool run that confirms the choice, with its pass condition.

When workload, sizes or versions are unknown, state the assumption and what would change the answer. Use a table to compare options on named criteria; use a sentence for a simple answer.

## Review findings

Order findings worst first. Give each its location (`path:line` or component), trigger, consequence, fix and the check that proves the fix, with its [hypothesis-test result](references/review-method.md#steps). Separate proven defects from candidates that still need a check or a measurement.

## Decision record

Consequential work only; include the material slots in any layout. Use Mermaid for wiring of three or more hops, with candidate edges marked. Each recommendation sits beside its evidence and the check that would change it.

- **Decision (ADR).** Context (problem, constraints, business requirements); requirements (functional and measurable [quality attributes](references/architecture.md#quality-attributes)); current architecture (components, dependencies, data flow, ownership); decision and rationale; alternatives with their trade-offs; evidence (tests, static analysis, metrics, measured baselines); risks (failures, migration, security, blast radius); acceptance checks; rollout and rollback; consequences (accepted costs, limits, review date). Status goes in the header `status`.
- **Low-risk change.** Slice, impact, verify.
- **Plan.** Slice; wiring (source -> transform -> boundary -> sink); scope; interface and invariants; contract and data flow; tests as Given–When–Then (given <state>, when <action>, then <result>) starting from the first failing case (absent, concurrent, replay); blast radius; rollout and revert; rejected options. For a risky plan, an optional premortem: assume it failed and list the reasons independently.
- **Review, objection versus concern.** One major objection (blocks; states the consequence) or none, then non-blocking concerns, verification and a verdict (block or approve).
- **Material replan or unresolved verification.** After Action Review: intended, happened, why different or still unknown, next check. A selected `octocode-workflows` procedure, when installed, supplies recovery detail.
- **Architecture findings.** Model; finding (boundary -> mechanism -> impact -> proof) with its hypothesis-test result and confidence; alternatives; refactor (seam, slices, preserved contract, rollback); hot path (workload, baseline, attributed cost).
- **Deep review.** Critical paths and reviewed concerns, healthy invariants to preserve, proven defects and candidates, AST/LSP coverage limits, runtime evidence, measured efficiency and memory where relevant, and justified keep, delete or unify decisions. Do not assign health or efficiency from source shape alone.

Keep one coverage ledger for consequential work: surface or scenario, evidence, check and result, remaining gap. Use the selected workflow’s ledger when one exists. Final results summarize the verified scope, failed or unverified checks and remaining work; keep process identities and handoff bookkeeping in linked scratch evidence.

## Presentation choices

Decision → boundary and evidence → alternatives → checks. Separate confirmed edges from candidate relationships.

| Format | Use when / why |
|---|---|
| `C4Context` / `C4Container` / `C4Component` | Explain one abstraction level at a time. |
| `flowchart` / `sequenceDiagram` / `architecture-beta` | Show dependencies / runtime calls / deployment topology. |
| `graph` / comparison table | Explore larger networks / compare consequences and trade-offs. |

Use the [shared format guide](references/report-output.md#choose-the-format); load its Mermaid or widget reference only for that syntax.

## Saved report

Save a result with a table, a diagram, 40+ lines or 5+ findings, or when the user or `AGENTS.md` asks; otherwise answer in chat.

Keep approved edits in the source tree. Save the report at `<workspace>/.octocode/octocode-engineer/<YYYY-MM-DD>-<slug>.md` and scratch files in `<workspace>/.octocode/octocode-engineer/tmp/` (`<workspace>`: the repository root, else the current directory; never inside an existing `.octocode/`). A path the user or a parent names wins; a scratch folder does not (link from there).

Use the [shared header](references/report-output.md#report-header). Accepted `status` values: `Proposed`, `Accepted`, `Needs work`.

Follow the header with the `#` title and a lead paragraph that states the answer. Put findings under `## Findings` in one table, most severe first, then add `## Decision` and `## Checks`:

| Location | Finding | Boundary | Consequence | Confidence | Change |
|---|---|---|---|---|---|

Confidence: `high`, `medium` or `low`; a candidate gets `low` and a Finding starting `Candidate:`. A design comparison without defects puts one row per option under `## Findings`, with its consequence and confidence.

After delivery, follow the [HTML offer](references/presentation.md#html-delivery). Use [report commands](references/report-output.md#render-and-retrieve) for accepted rendering and `--manifest` discovery.
