---
name: octocode-rfc-generator
description: "Write or review RFCs, design documents and migration proposals before implementation. Connect one decision to alternatives, validation, rollout, risks and rollback in a single document."
---

# Octocode RFC Generator

Produce one self-contained RFC: the decision and how to carry it out.

```mermaid
flowchart LR
  Q[Frame decision] --> R[Research state and options]
  R --> D{Deciding evidence?}
  D -- yes --> W[Write RFC with plan]
  D -- no --> B[Draft open choice, next check]
  W --> V[Verify]
  B --> O[Deliver RFC or draft; offer HTML]
  V -- "new evidence needed and available" --> R
  V -- "unresolved prerequisite" --> B
  V -- ready --> O
```

## One document

- Other deliverables only on explicit request ([output](output.md#delivery)).
- Link source code, authoritative docs, and external evidence; don't copy them.
- Plan-only request for a settled decision: one plan, no invented open decision.

## Research and decide

- Before drafting, list earlier RFCs with `--manifest` ([report commands](references/report-output.md#render-and-retrieve)); revise a selected one or mark it `Superseded`.
- Frame problem, users, goals, constraints, and current behavior; verify deciding facts with `octocode-research`.
- Decision (ADR): context, decision, status (header `status`), consequences. Compare viable alternatives (status quo when useful); each recommendation carries its because (evidence) and the check that would reverse it.
- Open deciding blocker: a direction with explicit open questions, never a final recommendation ([readiness](references/checklist.md#before-declaring-ready)).
- Plan: observable success first; order steps by dependency (`S2 consumer` after `S1 contract`); each step names what changes, its check, compatibility, rollout, rollback. Risky plan: optional Premortem (assume it failed; list reasons independently); each reason becomes a risk.
- Flow or boundary hard to state in prose: add an example or [diagram](references/checklist.md#diagrams); text keeps the meaning.
- Auditing an existing RFC: [workflow](references/workflow.md#audit-an-existing-rfc).

## Write, verify, deliver

- Write per [output](output.md): proposal and effect first, then design and tradeoffs.
- Check against current sources, dependencies, acceptance criteria, and project checks; `octocode-documentation` for readability. Report material gaps outside the RFC.
- An RFC does not authorize implementation; reuse the user's authorization for scoped saves and edits.
- Save the RFC per [output](output.md#saved-report), then offer HTML.

## Resources

| When | Read | Why |
|---|---|---|
| Repository evidence needs Octocode | [MCP/CLI guide](references/octocode-usage.md) | Discovery, invocation, complete reads, recovery |
| Planning evidence, auditing an RFC, or a disagreement survives source review | [workflow](references/workflow.md) | Evidence per question, audit, optional `clasify` |
| Finding or closing blocking questions; a section needs a diagram | [checklist](references/checklist.md) | Question areas, closure rules, readiness, diagram types |
| Saving, rendering, or listing RFCs | [render-report.mjs](scripts/render-report.mjs) per [output](output.md#saved-report) | HTML page, JSON catalog |

## Related skills

- Use `octocode-research` when option claims or current behavior need verification.
- Use `octocode-architect` when boundaries or structural tradeoffs decide the option.
- Use `octocode-brainstorming` when the issue or option space is still open.
- Use `octocode-documentation` for RFC language, or an ADR for a settled decision.
- Use `octocode-eval-benchmark` when deciding claims need measurement.
- Use [octocode-workflows composition](../octocode-workflows/references/composition.md) for an orchestrated multi-actor task.

## Output

[output.md](output.md): structure, language, delivery. Only the optional [clasify review](references/workflow.md#optional-clasify-review) needs a key.
