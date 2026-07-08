# Workflow: Change Mode

Use when the user asks to implement, refactor, migrate, or patch after evidence gathering. Read `algorithm.md` first for the router and evidence grades; use `code-research.md` for the proof ladder before editing.

```text
current contract + invariants
-> blast radius: callers, references, imports, tests, configs
-> existing local pattern to copy
-> patch boundary: smallest files/symbols that solve the claim
-> verify: targeted test/build/typecheck/lint/smoke or exact read when no runtime check exists
-> if failed: read the failing path, update the ledger, patch only the cause, or report blocked
```

Change rules:
- Ask before public contracts, cross-package edits, deletes/renames, or many consumers.
- Do not mix opportunistic cleanup with the requested patch.
- Final answer states patch scope, verification that ran, remaining gaps, and confidence.

If one pass does not converge — verification keeps failing or evidence keeps shifting — escalate to `loop-mode.md` instead of guessing further.

Validate: `node scripts/eval-research.mjs --case change-mode`.
