# Workflow: Change Mode

Use when the user asks to implement, migrate, or patch **behavior** after `references/problem-framing.md` defines the task class and success criteria.
For structure/name/module/layout reshapes that preserve behavior, use `references/workflow-refactor.md` instead.
Read `references/algorithm.md` first for the router and evidence grades; use `references/code-research.md` for the proof ladder before editing.

```text
problem contract + task class + acceptance/regression criteria
-> current contract + invariants
-> blast radius: graph dependents/path + LSP callers/references + tests/configs
-> existing local pattern to copy
-> patch boundary: smallest files/symbols that solve the claim
-> verify: targeted test/build/typecheck/lint/smoke or exact read when no runtime check exists
-> if failed: read the failing path, update the ledger, patch only the cause, or report blocked
```

Change rules:
- Bug fix: preserve supported contracts and add a regression check for the reproduced trigger.
- Feature: implement explicit acceptance criteria; name compatibility and rollout decisions.
- Enhancement: record baseline and target; verify the target plus existing regression guards.
- Ask before public contracts, cross-package edits, deletes/renames, or many consumers.
- Use graph reachability/cycles when an entrypoint, package boundary, or import rewire can change repository topology; inspect exact edges before patching.
- Do not mix opportunistic cleanup with the requested patch.
- Final answer states task class, criterion met, patch scope, verification that ran, remaining gaps, and confidence.

If one pass does not converge — verification keeps failing or evidence keeps shifting — escalate to `references/loop-mode.md` instead of guessing further.

Next: validate the landed diff with `references/workflow-pr-review.md`; when the cause is still unproven fall back to `references/workflow-debug.md`; when the work turns out to be structure-only hand back to `references/workflow-refactor.md`.
