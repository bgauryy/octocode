# Staged Approval

Use this before changing repo instructions, skills, harness guidance, memory-corpus notes, or any standing agent behavior.

## Stage Shape

Every proposal must name:
- Target files or surfaces.
- Failure/opportunity and why it matters.
- Evidence: user correction, test, eval, memory id, refinement id, source file, or runtime output.
- Proposed change.
- Risks and rollback.
- Verification plan.
- Approval status.

Template:

```text
Proposal:
Target:
Evidence:
Change:
Risk/Rollback:
Verification:
Approval needed:
```

## Storage

Use conversation for small one-turn proposals. Use `memory_reflect fix_harness:` for durable harness proposals. Use `memory_reflect fix_repo:` when the next repo run must see the follow-up in `memory_refine_get`.

Do not encode raw private reasoning, secrets, or long transcripts. Store pointers and concise evidence.

## Approval Gate

Before applying a staged proposal:
1. Confirm the user approved this exact scoped change.
2. Re-read the current target files.
3. Apply the smallest patch.
4. Run skill lint and relevant package tests.
5. Record verification in the final answer or with `memory_verify` if tied to an edit task.

One approval covers one scoped change. New target files, broader policy, or different behavior require a new approval.
