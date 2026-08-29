# Workflow: Refactor Mode

Use when reshaping names, modules, files, or directories while preserving behavior and contracts. Why: structural change needs identity and blast-radius proof, not feature logic.
Read `references/algorithm.md` first and use `references/code-research.md` for proof. New behavior routes to `references/workflow-change.md`; contested architecture routes to an explicit design doc before coding.

Flow: `SKELETON → CONTRACTS → BLAST → PLAN → EXECUTE → VERIFY → CLEAN`.

## Size once
- S: up to three files or one symbol — exact patches, LSP references, targeted test.
- M: one package/module — task ledger, semantic rename, package checks.
- L: cross-package/tree — bulk moves, proven path rewrites, layered verification.
Scale execution, not evidence quality.

## Proof before edits
1. Skeleton: map roots/hotspots with structure/files; use graph dependencies/dependents/reachability for file topology and inspect symbols on entry points/move targets.
2. Contracts: freeze public exports, types, schemas, flags, tool names, env keys, serialized shapes, tests, and package boundaries.
3. Blast: combine graph dependents/path/cycles with semantic references/callers and lexical/structural search across code, tests, scripts, configs, and docs.
4. Gate: confirm the plan before public rename/delete, cross-package edges, or many consumers.
Record one invariant list and a task ledger with files, contract risk, verification, and rollback.

## Execute big to small
1. Move directories/files with real move operations; avoid rewrite-by-copy.
2. Rewrite path literals only from a proven hit list; dry-run or spot-check first.
3. Rename symbols through semantic identity and exact patches; never blind-replace identifiers.
4. Clean internals only inside moved units; avoid unrelated drive-bys.
5. Re-run discovery and contract checks after every batch; stop on unplanned hits.
Shared repository: declare wide moves and path ownership before parallel edits.

## Verify and report
- S: targeted unit/type check.
- M: package test plus typecheck/lint.
- L: graph dependents/cycles/reachability, root build, diagnostics on moved roots, and final search for old paths/names.
On failure, read the failing path and patch only the cause; shifting evidence routes to `references/loop-mode.md`.
Delete only after the dead-code proof in `references/code-research.md`.

```text
Mode/Tier: {refactor / S|M|L}
Invariants: {preserved or explicitly changed contracts}
Changes: {moves, renames, scoped cleanup}
Verification: {commands and exit codes}
Confidence/Next: {level and remaining action}
```

Next: when behavior has to move hand off to `references/workflow-change.md`; before merging the reshape run `references/workflow-pr-review.md` over the diff; when verification keeps failing load `references/loop-mode.md`.
