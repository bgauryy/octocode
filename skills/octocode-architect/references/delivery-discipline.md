# Delivery Discipline

Load when Code or Review is shipping a slice. Why: implementation is incomplete until its behavior and repository state agree.

## Verification

- Start with a failing assertion on the owned interface; verify that it fails for the intended reason.
- Exercise the production path. Do not stub the dependency whose integration or behavior the test claims to prove.
- Run the narrowest relevant checks, then expand according to affected scope. Compare results with the recorded baseline, and inspect the final diff.
- Fail reachable unfinished paths explicitly; clean up acquired resources; preserve generated-code ownership.
- For optimization, record the metric, and baseline, change one variable, rerun comparably, and keep only measured improvement.

## Cleanup

Leave touched territory clean, healthy, and maintainable. Remove dead imports and obsolete code created or exposed by the change; fix affected formatting, stale comments, naming drift, and minor structural clutter when the cleanup is safe and directly related.

Do not broaden the task into a refactor. If cleanup changes public behavior, ownership, architecture, or meaningful review scope, report it as separate work instead of smuggling it into the slice.

## Bookkeeping

Discover repository conventions rather than updating every possible record. When required by the implementation or release contract, update the authoritative changelog, version, documentation, manifest, schema, generated file, lockfile, fixture, or snapshot.

Regenerate derived artifacts from their source; never hand-edit generated output. Avoid blanket version bumps, lockfile churn, snapshot acceptance, or changelog entries without a repository-specific reason.

## Definition of done

1. The owned interface behaves as intended on normal and named edge paths.
2. Impacted callers, data paths, operations, and rollback assumptions were checked. <!-- style-lint: ignore-line passive-voice -->
3. Relevant tests and sensors passed, or remaining failures are classified and reported with attribution evidence.
4. Task-scoped cleanup is complete and unrelated cleanup is excluded. <!-- style-lint: ignore-line passive-voice -->
5. Required bookkeeping matches the code; no stale derived or descriptive state remains.

Next: use `output-contracts.md` to report a consequential result; otherwise return a concise outcome and verification summary.
