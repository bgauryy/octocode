# Verify Awareness

Verification must cover the source contract, built artifacts, and the affected host path. Compilation alone is insufficient.

## Package gate

Run the package checks:

```bash
yarn workspace @octocodeai/octocode-awareness lint
yarn workspace @octocodeai/octocode-awareness test
yarn workspace @octocodeai/octocode-awareness pack:check
```

Do not lower coverage thresholds to pass a change. Rebuild before testing the generated CLI and skill assets.

## Built surface gate

Verify the built CLI reports five concepts and the same operations as `ROUTINE_AWARENESS_OPERATIONS`:

```bash
node packages/octocode-awareness/out/octocode-awareness.js --help
node packages/octocode-awareness/out/octocode-awareness.js \
    schema commands --compact
```

Verify an unknown or removed operation fails. Import the built package root and assert that its runtime keys are exactly:

- `AWARENESS_CONCEPTS`
- `AWARENESS_AGENT_INSTRUCTION_SECTIONS`
- `AWARENESS_MESSAGE_PARAMETER_GUIDANCE`
- `ROUTINE_AWARENESS_OPERATIONS`
- `createAwarenessClient`
- `getAwarenessAgentInstructions`
- `getAwarenessOperationDescriptor`
- `listAwarenessOperationDescriptors`

Import `/schema`, `/host`, and `/admin` separately. Confirm `/admin` has exactly the three migration functions and that routine discovery contains no host capture or migration operation.

## Workflow gate

Use a disposable database and workspace to verify:

1. `context.orient` returns a bounded packet and a reusable revision.
2. Work creation, claim, dependency, transition, and observed verification share the same IDs.
3. Message send, reply, list, and resolution preserve the thread identity.
4. Memory recall returns scoped evidence and exposes any terminal limit.
5. History timeline and read expose executable continuations.
6. Restore preview detects drift; authorized apply returns verification debt.
7. A real exclusive protection blocks a conflicting host write.
8. Infrastructure failure records a degraded hook receipt without mislabeling a real conflict.
9. Keyed memory preserves historical rationale, rejects competing revisions, and returns bounded anchor matches.
10. Changed declared file evidence becomes stale without certifying a lesson; scoped orientation refreshes its revision.
11. Non-file experiences paginate without loss, seal durably when LocalGit is available, and remain readable after delivery pruning.

Execute every continuation in the fixture and verify that the union of pages covers the expected rows.

Run the build-generation coherence test. Runtime entries must remain self-contained and keep already-loaded lazy modules usable across a rebuild; hashed executor chunks must not be emitted.

## Host gate

For a Pi change, verify native ownership is claimed and the shell runner performs no identity, database, or receipt work. For a shell adapter change, verify the inverse policy and one real lifecycle event.

Keep these claims separate in the evidence:

- The integration contract exists.
- Host configuration selects it.
- The running host activates it.
- The target database contains the expected receipt.

## Migration gate

For a predecessor database:

1. Preview into a different absent destination.
2. Confirm the preview reports every source relation and no unexplained omission.
3. Apply copy-on-write and prove the source digest is unchanged.
4. Independently verify schema, integrity, foreign keys, counts, event order, store ID, and available LocalGit reachability.
5. Stop writers, recheck the source digest, retain a rollback copy, and perform explicit cutover.
6. Run `context.orient` against the selected canonical store.

Never modify the source database in place.

## Architecture gate

Run dependency-cycle and dead-code analysis from all public entry points: package root, `/schema`, `/host`, `/admin`, CLI, hook runner, and hook file extractor. Confirm every deletion candidate with symbol references before removing it.

Finish with a lexical scan for every removed public symbol and command spelling. Migration code and predecessor fixtures can retain predecessor terminology only when the context is explicit.
