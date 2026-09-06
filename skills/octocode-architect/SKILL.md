---
name: octocode-architect
description: "Use when root-cause analysis, boundary design, blast-radius mapping, architecture review, or interface contracts matter."
---

# Octocode Architect

Work in small, evidence-backed slices. Treat code as wiring: data enters through an interface, changes shape under explicit invariants, crosses boundaries, and produces observable effects.

Flow: `THINK → PLAN → CODE → REVIEW`.

## Rules

- Name the use case and the smallest useful slice before editing. No use case or unresolved contract → stop and surface the gap.
- Decompose before solving: split the task into meaningful problems, map dependencies, run independent work concurrently when useful, verify each piece, then compose the result.
- Inspect the implementation, callers, runtime wiring, data flow, tests, and similar behavior. Description is not evidence.
- Map affected scope and impact before the write: interfaces, consumers, persisted data, operations, tests, and rollback. Recheck them in the diff afterward.
- Before editing, inspect the working tree and record comparable baseline checks. Existing changes may belong to a human or another agent: preserve them; never stash, reset, overwrite, or discard them; coordinate overlapping paths before writing.
- After editing, rerun the same or directly comparable checks and classify each failure as pre-existing, introduced by your change, introduced by concurrent work, or uncertain. Fix only in-scope failures; report the others with attribution evidence.
- Scale rigor to material risk. Do not invent layers, alternatives, findings, or operational work to fill a checklist.
- Prefer the strong design: cohesive boundaries, explicit types, caller-shaped interfaces, no duplicate sources of truth.
- Use the smallest slice that gives fast feedback. Mechanical work uses mechanical tools; judgment stays explicit.
- Improvement needs a sensor: baseline → change one thing → rerun → compare.

## Workflow

1. **Think** — map source → transformation → boundary → sink and identify the owned interface. For consequential design, load `references/architecture-lenses.md`; it defines the system views and impact map.
2. **Plan** — name In, Out, Interface, Test, Edges, Touches, dependencies, parallel work, and material risks. Use `references/output-contracts.md` when a written plan or review contract can improve the decision.
3. **Code** — before editing, load `references/delivery-discipline.md`; it owns verification, cleanup, and definition-of-done rules. Write the failing surface test, implement one slice, and exercise the production path. Do not route around a wrong model or boundary.
4. **Review** — inspect the diff and rerun focused checks. Complete required cleanup and bookkeeping.

## Gates

- Ask before coding when the use case, contract, ownership, or migration choice is unsettled; otherwise proceed. <!-- style-lint: ignore-line passive-voice -->
- Cleanup is limited to cleanup caused by or directly adjacent to the change. Never turn a task into an unsolicited refactor. <!-- style-lint: ignore-line passive-voice -->
- Bookkeeping follows the repository and release contract: update only required docs, manifests, versions, generated artifacts, lockfiles, changelogs, or snapshots.
- Unimplemented reachable paths fail explicitly. Tests prove outcomes, not merely calls.
- Chat-only output stays in chat. Requested source edits stay in their named repository; do not create planning artifacts unless asked.
