---
name: octocode-rfc-generator
description: "Write or review RFCs, design documents and migration proposals before implementation. Connect one decision to alternatives, validation, rollout, risks and rollback in a single document."
---

# Octocode RFC Generator

Keep the decision and its implementation plan in one RFC. Produce additional deliverables only on explicit request. For a settled decision and a plan-only request, write one plan. An RFC does not authorize implementation.

1. **Find prior decisions.** Before drafting, list earlier RFCs with `--manifest` ([report commands](references/report-output.md#render-and-retrieve)). Revise a selected RFC or mark it `Superseded` when replacing it. For review, [audit the existing RFC](references/workflow.md#audit-an-existing-rfc) against current code. Return findings for review-only requests. Reuse existing authorization for scoped saves and edits.
2. **Resolve the choice.** Frame the problem, affected users, current behavior, goals and constraints. Use the [evidence workflow](references/workflow.md) and [MCP/CLI guide](references/octocode-usage.md) for deciding facts. Compare viable alternatives, including the status quo when useful. Record the ADR context, decision, header `status` and consequences. Link each recommendation to its evidence and the check that would reverse it.
3. **Plan from observable success.** Order steps by dependency (contract before consumer). For each, name the change, compatibility, check, rollout and rollback. For a risky plan, a Premortem can turn independent failure explanations into risks. Use an example or [diagram](references/checklist.md#diagrams) for a difficult flow or boundary. Keep the meaning in the text.
4. **Check readiness.** Verify sources, dependencies, acceptance criteria and project checks. Revisit evidence when a useful check remains. Apply the [closure and readiness rules](references/checklist.md#before-declaring-ready). An unresolved deciding blocker yields a provisional direction with open questions and next checks. Only a consequential disagreement that survives source review and lacks a direct test may warrant [optional `clasify` review](references/workflow.md#optional-clasify-review), which alone needs a classification key.

## Output

Follow [output.md](output.md) for the required sections and [delivery](output.md#delivery). Lead with the proposal and its effect. Link original sources and report material verification gaps in the handoff. [Save every RFC](output.md#saved-report), then offer HTML. Use [render-report.mjs](scripts/render-report.mjs) for accepted rendering or catalog discovery.

## Related skills

Use `octocode-research` to verify current behavior and option claims, `octocode-architect` when a boundary decides the choice, and `octocode-brainstorming` when the option space is open. Use `octocode-documentation` for readability or a settled ADR, `octocode-eval-benchmark` for deciding measurements, and [workflow composition](../octocode-workflows/references/composition.md) for multi-actor work.
