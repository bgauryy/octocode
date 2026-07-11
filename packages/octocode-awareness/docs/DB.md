# Awareness database

Awareness stores runtime coordination state in one SQLite database. The default
path is `$OCTOCODE_MEMORY_HOME/awareness.sqlite3`; without that environment
variable it uses the platform memory-home directory resolved by
`src/db-runtime.ts`.

`<workspace>/.octocode/` is not the database. It contains generated projections
for readers that cannot query Awareness directly. SQLite remains authoritative.

## One executable contract

`src/db-schema.ts` owns all table, index, and optional FTS DDL. There is one
current contract and one application identity:

```text
application_id = 0x4f435431  # ASCII OCT1
```

The application ID distinguishes Awareness from unrelated SQLite files. The
normalized schema fingerprint distinguishes the exact executable contract.
There is no second initializer or parallel numeric contract field.

The public database layer is split by responsibility:

- `db-runtime.ts` opens connections, classifies stores, chooses journal mode,
  applies retry bounds, and exposes cached connections.
- `db-init.ts` serializes first initialization and creates the contract.
- `db-schema.ts` contains the executable DDL.
- `db-introspection.ts` derives the expected relation set and fingerprint.
- `db-search.ts` owns FTS and search-adjacent helpers.
- `db.ts` is the public barrel.

## Startup contract

Startup accepts only two states:

1. An empty SQLite store, which is initialized from the canonical DDL.
2. An OCT1 store whose relations and normalized fingerprint match exactly.

Any other application ID, unbranded non-empty store, missing relation, extra
relation, or changed DDL is rejected before Awareness writes application data.
This fail-closed boundary prevents the package from guessing ownership or
silently reshaping an incompatible database.

Fresh initialization runs under `BEGIN IMMEDIATE`. A second process that opens
the same empty path waits on the bounded SQLite busy retry, reclassifies the
store after acquiring the write lock, and observes the completed contract. The
application ID is written only after DDL, indexes, optional FTS, fingerprint,
integrity, and foreign-key checks succeed.

`initDb(db)` rejects caller-owned transactions because it must own that complete
serialization boundary. `connectDb(path)` is the normal file-backed entry point.

## SQLite runtime safety

The embedded SQLite library version controls journal selection:

- Runtime builds known to be safe use WAL for concurrent readers and writers.
- Other builds use rollback journaling.

This is a runtime capability check, not a database contract number. Both paths
set a bounded busy timeout and use the same retry deadline around journal mode
and first initialization.

Foreign keys are enabled on every returned connection. Initialization briefly
disables connection-local enforcement while creating the complete empty
contract, then restores it before returning.

## Integrity and fingerprint checks

The canonical fingerprint covers tables, named indexes, views, triggers, and
the optional `memories_fts` virtual table. SQLite-generated internal objects and
FTS shadow tables are excluded.

Initialization and canonical opens enforce:

- the complete expected relation set;
- no unexpected application relations;
- normalized DDL equality;
- `PRAGMA integrity_check`;
- `PRAGMA foreign_key_check`.

An exact fingerprint means a DDL edit is a contract change. Coordinate such a
change explicitly and create a fresh store; do not add an alternate initializer
or a numeric field that allows two definitions to coexist.

## FTS

FTS5 is optional because the embedded SQLite build may omit it. When available,
`memories_fts` is created from `FTS_SCHEMA_DDL` and rebuilt from the empty
canonical memory tables during initialization. Search helpers detect its
presence at runtime and retain non-FTS behavior when unavailable.

## Operational checks

Database-facing CLI results have one shape per action. `work list|show` returns
flat work rows for direct inspection; Attend's `FilesUnderWork` groups those
rows for coordination summaries.

Use the package CLI and tests rather than editing the database manually:

```bash
yarn workspace @octocodeai/octocode-awareness test
yarn workspace @octocodeai/octocode-awareness test:smoke
yarn workspace @octocodeai/octocode-awareness lint
```

The concurrency contract is covered by `tests/concurrent-init.test.ts`. Schema
identity, drift rejection, idempotence, FTS behavior, and delivery-state helpers
are covered by `tests/schema.test.ts`.

If a store is rejected, preserve it for inspection and point Awareness at a new
path. Automatic transformation of an unrecognized store is intentionally
outside the runtime contract.
