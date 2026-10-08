# RFC workflow

Use to choose the deliverable or audit an existing proposal. [output.md](../output.md) owns the single-file structure and language.

## Choose the result

- An open consequential decision produces one RFC with its execution plan inside it.
- A settled decision that needs only implementation steps produces one plan when that is the user's request.
- A review returns findings, or updates the existing RFC when edits are requested. Preserve accepted history and make the current decision clear.

## Work through the dependencies

1. Verify the current state, problem, and constraints. Investigate alternatives that could realistically meet the goal.
2. Resolve deciding blockers before presenting a final recommendation. A blocked draft explains the open choice and next useful check.
3. Define acceptance criteria, then write steps in dependency order. A step consumes an available prerequisite or an earlier step's result.
4. Explain material compatibility, security, migration, and rollback implications with an accountable owner where needed.
5. Review the whole argument for evidence, consistency, and readable language. Deliver only the RFC and a concise handoff.

Keep prerequisites, success metrics, implementation, and references as sections in the same file. Working notes and tool records are internal research material; never append them to the RFC or create sidecar reports by default.

## Audit an existing RFC

Read the proposal and any existing supporting material, then verify the live code it describes. Identify implemented work, wanted gaps, contradictions, or superseded choices. Explain whether to keep, update, or retire it with deciding evidence.

For authorized edits, update substantive content in the existing RFC while preserving accepted decisions. Do not add audit headers, probe transcripts, or generated review metadata. Archive or delete only within the user's existing scope, and update affected references.

Next: [research](research-playbook.md), [completeness](rfc-completeness.md), and [output](../output.md).
