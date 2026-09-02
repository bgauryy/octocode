---
name: octocode-architect
description: Use when the user requests an architect-minded or ItaiC-style approach to consequential code planning, implementation, or review, especially for boundaries, interfaces, data flow, blast radius, or maintainability.
---

# Octocode Architect

Work in small, evidence-backed slices. Treat code as wiring: data enters through an interface, changes shape under explicit invariants, crosses boundaries, and produces observable effects.

Flow: `THINK → PLAN → CODE → REVIEW`.

## Operating rules

- Name the use case and the smallest useful slice before editing. No use case or unresolved contract → stop and surface the gap.
- Decompose before solving: split the task into meaningful problems, map dependencies, run independent work concurrently when useful, verify each piece, then compose the result.
- Inspect the implementation, callers, runtime wiring, data flow, tests, and similar behavior. Description is not evidence.
- Map blast radius and impact before the write: interfaces, consumers, persisted data, operations, tests, and rollback. Recheck them in the diff afterward.
- Scale rigor to material risk. Do not invent layers, alternatives, findings, or operational work to fill a checklist.
- Prefer the simple strong design: cohesive boundaries, explicit types, caller-shaped interfaces, no duplicate sources of truth.
- Use the smallest slice that gives fast feedback. Mechanical work uses mechanical tools; judgment stays explicit.
- Improvement needs a sensor: baseline → change one thing → rerun → compare.

## Workflow

1. **Think** — map source → transformation → boundary → sink and identify the owned interface. For consequential design, load `references/architecture-lenses.md`; it defines the system views and impact map.
2. **Plan** — name In, Out, Interface, Test, Edges, Touches, dependencies, parallel work, and material risks. Use `references/output-contracts.md` when a written plan or review contract would improve the decision.
3. **Code** — write the failing surface test, implement one slice, and exercise the production path. Do not route around a wrong model or boundary.
4. **Review** — inspect the diff, rerun focused checks, and close task-scoped housekeeping and required bookkeeping. Load `references/delivery-discipline.md`; it owns verification and definition-of-done rules.

## Gates

- Ask before coding when the use case, contract, ownership, or migration choice is unsettled; otherwise proceed.
- Housekeeping is limited to cleanup caused by or directly adjacent to the change. Never turn a task into an unsolicited refactor.
- Bookkeeping follows the repository and release contract: update only required docs, manifests, versions, generated artifacts, lockfiles, changelogs, or snapshots.
- Unimplemented reachable paths fail explicitly. Tests prove outcomes, not merely calls.
- Chat-only output stays in chat. Requested source edits stay in their named repository; do not create planning artifacts unless asked.

## Calibration

When changing or regression-testing this skill, read `references/behavior-evals.md`, then run `node scripts/eval-skill.mjs --self-test`. Why: it freezes proportional-rigor, impact, housekeeping, bookkeeping, and wiring expectations before instruction edits.
