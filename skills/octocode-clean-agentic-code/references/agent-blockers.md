Load when agent-written code masks an error, leaves a reachable stub, adds an unverified dependency, or exposes a credential or insecure setting: report it, never delete it.

# Agent blockers

## Report, never delete

Removal would reveal a wrong result → [escalation protocol](#escalation-protocol). Error masking (category `error-masking` rows find the literal forms):

- `log-only-catch`: a handler that logs and continues, no rethrow or user signal. Its log line is part of the masked failure, not a debug print.
- `empty-catch`: a swallowed error with an empty handler.
- `empty-fallback`, or a default (`?.`, `?? <literal>`, `.get(k, default)`) on a value that is never legitimately absent, or a hardcoded sample response after a failed call.
- Success returned without checking the effect landed (unchanged bytes, rows, requests): highest severity.
- `not-implemented-stub`, or a `pass`, `return None`, `return {}` body, on a reachable path.
- Retry or sleep loop that swallows a failed call or has no bounded termination and failure signal.

Supply chain and credentials (verify):

- Dependency missing from the registry: `artifactSearch` `operation: "resolve"` with its `ecosystem` and exact `packageName` answers `errorCode: "notFound"` (any other error proves nothing); attacker-claimable, not a typo, until a maintainer confirms.
- Plausible, recent, near-zero-adoption package: compare with the package it imitates.
- Import absent from every manifest: deps-usage `undeclared`, or the graph `undeclared-dependency` detector; declare or drop it per [Dependencies](types-dependencies.md#dependencies).
- `placeholder-credential`, or a placeholder URL for a real integration: never ran against a real credential.
- `credential` (scan-signals prints it redacted; fixtures are scanned for it too): redact, report for rotation; history removal is separate.
- `mutable-ref` (branch or major-only action ref, untagged or latest image, `*`/`latest` version, unpinned global install): record the intended version with the owner.
- `insecure-setting` (TLS verification off, privileged container, world-writable mode, write-all token): the owner picks the secure value.

## Escalation protocol

1. Report blocking defects in SBAR: masked failure and location, intended behavior, affected consumers and recommended repair. Use existing evidence; request more investigation only for a material gap.
2. Keep it out of every batch, approved ones included.
3. Confirmed test-integrity finding → clean the test per [tests](tests.md) only after the bug is fixed.

## Prevention

- An instruction file that forbids unverified dependencies proves nothing about the diff: run the registry check above on every dependency the change adds.
- After a patch tool edits files, read each edited file's diff: the intended change is present and its indentation matches the surrounding block.
