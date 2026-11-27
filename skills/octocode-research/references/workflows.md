Load for an authorized behavior change, refactor or deletion.

# Prove, then change

In every change, keep coverage floors and never edit a grader to hide a failure. Report per [output](../output.md#response).

## Behavior change

1. Name the contract, trigger, consumers and smallest affected boundary. Read source and local patterns; use graph/LSP impact when symbols or imports cross boundaries.
2. Find or write a regression/acceptance check and confirm it fails for the intended reason before patching. Freeze baseline and target for an enhancement. A trivial reversible edit needs a direct check, not a test mirroring its text.
3. Make the smallest coherent change and pass the same check at the changed boundary; refactor within scope while it stays green.
4. Update consumers and remove obsolete owned paths. Avoid shims, legacy aliases or duplicate owners unless required; preserve unrelated edits. After a tool/package change, rebuild and exercise the affected CLI/MCP interface.

## Refactor

Map structure, entry points, move targets, preserved contracts and authorized interface changes. Discover affected users with graph dependents/path/cycles, LSP references/callers and text/AST across code, tests, scripts, configs and docs. Move rather than copy; derive path edits from proven hits and renames from semantic identity.

After each batch, rerun discovery. Unexpected users reopen the impact map. Choose checks by affected contracts:

| Scope | Verify |
|---|---|
| One behavior boundary | Targeted check |
| Package contracts | Package tests, typecheck and lint |
| Cross-package contracts | Dependents/cycles, affected builds and final search for old names |

## Deletion

When the graph suggests an unused export, inspect `lspSearch` callers and read the call sites. Before deletion also check runtime registration, tests, configuration and external consumers. Name any scope you could not verify.
