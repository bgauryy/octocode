# Change and refactor

Load when implementing behavior after authority and success criteria are clear, or reshaping code while preserving behavior.

## Behavior change: RED → GREEN → REFACTOR
1. Name the contract, trigger, consumers, and smallest behavior boundary.
2. Write or identify a regression/acceptance check and observe it fail before the patch; tell intended failure from broken setup.
3. Implement the smallest coherent change; rerun the same check to green; refactor inside that scope while green.
4. Run the lobby `VERIFY` checks; after a tool/package change, rebuild the CLI/MCP before that run.

Enhancements freeze a baseline and target first. A trivial reversible edit needs a direct check, not a test that mirrors its text.

## Design rules
- Exact reads and local patterns before patching; graph/LSP impact checks when imports or symbols cross boundaries.
- No compatibility shims, legacy aliases, or duplicate paths unless explicitly required; remove obsolete owned paths, update consumers, preserve unrelated edits.
- One owner per public contract.

## Refactor: SKELETON → CONTRACTS → BLAST → EXECUTE → VERIFY
Size: S ≤ 3 files or one symbol · M one package · L cross-package. Scale execution, not evidence quality.
1. Skeleton: structure, graph dependencies/dependents, symbols on entry points and move targets.
2. Contracts: behavior to preserve; interfaces authorized to change.
3. Blast: graph dependents/path/cycles + LSP references/callers + text/AST across code, tests, scripts, configs, docs.
4. Execute big → small: real moves (not copies), path literals from a proven hit list, renames via semantic identity. Re-run discovery after each batch; stop on unplanned hits.
5. Verify: S targeted check · M package test + typecheck + lint · L graph dependents/cycles, root build, final search for old names. Delete only after the lobby safe-delete proof.

Report: `Mode/Tier · Invariants · Changes · Verification (commands, exit codes) · Confidence/Next`.

Next: final diff review → `workflow-pr-review.md`; repeated verification failure → `campaigns.md`.
