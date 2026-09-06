# Awareness database

Awareness owns coordination state in an Awareness-only SQLite database. The
default is `$OCTOCODE_HOME/awareness/awareness.sqlite3`; an explicit workspace
policy or `--db-scope repo` selects `<workspace>/.octocode/awareness.sqlite3`,
while `--db` has highest precedence. [STORAGE_SCOPES.md](STORAGE_SCOPES.md) defines placement,
overrides, artifacts, and legacy migration.

The normal Awareness store opener never opens
`$OCTOCODE_HOME/agent/agent.sqlite3` or
`$OCTOCODE_HOME/agent/core.sqlite3`. Those files belong to the Agent. The
explicit legacy migration reader can open a recognized mixed `agent.sqlite3`
read-only as a migration source; it never treats that file as an Awareness
target. A database owned by the Octocode CLI, an MCP server, or another
`.octocode/` consumer is also outside this package's authority.

## Entity ownership

Awareness owns collaboration entities such as:

- plans, plan membership, tasks, dependencies, claims, and task runs;
- advisory file work, exclusive locks, and verification receipts;
- agents, messages, delivery state, signals, and coordination handoffs;
- memories and references, refinements, reflection, and maintenance records;
- Awareness hook receipts, Awareness-specific session captures, and the
  redacted `worker_lifecycle_events` coordination projection.

These records describe coordination. The worker projection is not the Agent's
authoritative worker mailbox, worktree/handoff ledger, process state, or
dependency ledger. Agent runtime sessions, effects, lifecycle records,
automation leases, and fencing state belong in
`$OCTOCODE_HOME/agent/core.sqlite3`; Agent control/index data belongs in
`$OCTOCODE_HOME/agent/agent.sqlite3`.

Don't copy table totals into prose. The executable relation contract changes as
entities evolve:

- `src/db-schema.ts` owns the canonical tables, indexes, and optional FTS.
- `src/db-continuity-schema.ts` defines the unique delivery and interaction tables.
- `src/db-introspection.ts` derives the expected relation set and schema
  fingerprint.
- `tests/database-shared-contract.test.ts` and
  `tests/database-advanced-contract.test.ts` assert the executable schema.

Use `schema commands --compact`, `schema entities --compact`, and
`schema json-schema <name>` for public CLI and entity contracts. Pass `--all` to
`schema entities` for owner and relation kind. Treat those schemas and the
executable DDL as authoritative when this page and implementation diverge.

## Query ownership

For primary keys, foreign keys, logical references, and host-owned identifiers,
see [entity identities and connections](ENTITY_LINKS.md).

`AWARENESS_QUERY_VIEWS` in `src/repo-model.ts` enumerates live views.
`src/repo-query.ts` dispatches them to focused row builders:

| Views | Owner |
|---|---|
| `repo-profile`, `files`, `activity` | `src/repo-files.ts` |
| `memories`, `gotchas`, `lessons`, `plans`, `tasks`, `runs` | `src/repo-plans.ts` |
| `locks`, `agents`, `signals`, `refinements`, `developer-review` | `src/repo-coordination.ts` |
| `workboard` | `src/repo-workboard.ts` |
| `all` | `src/repo-query.ts` bounded fan-out |

Formatting lives in `src/repo-formats.ts`, scoping in `src/repo-scope.ts`, and
explicit export writes in `src/repo-projection.ts`. Query exports are read-only
snapshots; Awareness never reads them back as canonical state.

## Fail-closed database identity

Agent and Awareness databases have distinct application identities and expected
relation sets. An Awareness open accepts an empty database or a recognized
Awareness database. It rejects:

| Store | `PRAGMA application_id` | ASCII |
|---|---:|---|
| Agent control/index | `0x4f435441` | `OCTA` |
| Agent Rust runtime | `0x4f434147` | `OCAG` |
| Awareness coordination | `0x4f435431` | `OCT1` |

- an Agent control or Agent runtime identity;
- a CLI, MCP, or other foreign application identity;
- an identity-free database with unrecognized relations;
- a drifted Awareness schema with unknown or incompatible relations; and
- unexpected application tables, views, indexes, or triggers.

Recognized historical Awareness relation subsets are upgraded additively in one
transaction. Identity validation happens before schema writes. Initialization
assigns the Awareness identity only after DDL, indexes, optional FTS,
fingerprint, integrity, and foreign-key checks succeed. It must never relabel a
populated database to make it appear compatible.

Fresh initialization runs under a write transaction so concurrent first opens
serialize. A waiting process reclassifies the database after acquiring the lock;
it doesn't assume the file is still empty.

## SQLite runtime safety

The embedded SQLite version controls journal selection. Runtime builds known to
be safe can use WAL for concurrent readers and writers; other builds use rollback
journaling. Both paths use a bounded busy timeout and retry deadline.

Every returned connection enables foreign keys. Initialization and canonical
opens enforce:

- the complete expected Awareness relation set;
- no Agent-owned or otherwise unexpected application relations;
- normalized DDL and schema-fingerprint equality;
- `PRAGMA integrity_check`; and
- `PRAGMA foreign_key_check`.

FTS5 is optional because the embedded SQLite build can omit it. When present,
the executable DDL and introspection modules own its virtual table and generated
shadow-table treatment.

## Operational checks

Use the Awareness CLI and package tests rather than editing a database manually:

```bash
npx @octocodeai/octocode-awareness maintenance init --workspace "$PWD" --compact
npx @octocodeai/octocode-awareness status --workspace "$PWD" --compact
yarn workspace @octocodeai/octocode-awareness test
yarn workspace @octocodeai/octocode-awareness test:smoke
yarn workspace @octocodeai/octocode-awareness lint
```

For an explicit file, use an Awareness-specific name:

```bash
npx @octocodeai/octocode-awareness maintenance init --db /absolute/path/awareness.sqlite3 --compact
```

For an old mixed Agent/Awareness database, use the explicit relocation command
documented in [STORAGE_SCOPES.md](STORAGE_SCOPES.md); don't point an Awareness
opener at the mixed source.

If identity or fingerprint validation rejects a store, preserve it for
inspection. Don't rerun initialization against an Agent, CLI, MCP, unknown, or
mixed database, and don't change `PRAGMA application_id` manually. Follow the
migration procedure in [STORAGE_SCOPES.md](STORAGE_SCOPES.md).

## Explicit consolidation copy

Use consolidation only to move a recognized historical Awareness ledger into a
**new** canonical file. It opens the source read-only, writes a private
temporary file beside the requested destination, validates it, then publishes
the finished file atomically. It never upgrades, relabels, or deletes the
source. The destination must not already exist.

```bash
npx @octocodeai/octocode-awareness database consolidate \
  --source /absolute/path/old-awareness.sqlite3 \
  --destination /absolute/path/awareness-consolidated.sqlite3 \
  --unattributed-agent-id migration-reviewer \
  --compact
```

`--unattributed-agent-id` is an explicit adoption choice for rows that lack an
actor. It does not fill in missing plan goals, document directories, task
reasoning, acceptance criteria, leases, or verification evidence. Review the
copy before selecting it as an Awareness database.

The command prints JSON. A safe refusal has `ok: false`; incomplete historical
data uses `error_code: "INCOMPLETE_SOURCE_CONTRACT"` and includes `issues`
with each affected `table`, row `id`, and `missing` fields. Repair those fields
in a separate reviewed copy of the old database, then run consolidation again
from that copy. Do not repair the original merely to make an opener accept it.

The programmatic API is
`consolidateDatabase(sourcePath, destinationPath, { unattributedAgentId })`.
On success it reports copied-table counts and adopted agent IDs. On failure it
leaves the source unchanged and does not publish a destination.
