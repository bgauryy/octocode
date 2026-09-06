# Awareness architecture

`@octocodeai/octocode-awareness` owns durable coordination, memory, hooks,
reflection, and recovery for coding agents. It remains independent of
`@octocodeai/agent-core`; hosts translate Awareness facts through adapters.

## Ownership

- Root domain modules own plans, tasks, work presence, locks, signals, memory,
  agents, and verification. `src/coordination/` adapts those owners for hosts
  and owns handoffs and continuity; it has no separate lifecycle or store.
- `src/db-runtime.ts`, `src/db-schema.ts`, `src/sql/`, and schema modules own
  SQLite opening, identity, statements, and maintenance. Explicit database
  conversion writes a new store; opening a database never migrates it.
- `src/attend-*`, signals, refinements, sessions, query, digest, reflection, and
  maintenance modules own the advanced operating and learning workflows.
- `bin/` owns CLI parsing and presentation; domain behavior remains in `src/`.
- The package-local `skills/octocode-awareness/` directory is the canonical
  skill source. Generated helpers, `out/skills/`, and `.agents/skills/` are build output.
- `@octocodeai/agent-contracts` owns Agent control-database paths and tables,
  low-level SQLite utilities, shared entity types, and cross-host protocol
  fragments. Shared types and utilities don't imply shared physical storage.

During root development, the root `file:../octocode-agent/packages/octocode-agent-contracts`
resolution supplies a snapshot of the sibling shared-contract build. After rebuilding
that contract, refresh the snapshot with `yarn install`. The published package keeps
its semver development dependency and has no npm runtime dependencies. The sibling
native host imports the public `@octocodeai/octocode-awareness` API through a local
Yarn portal. Pi uses the Awareness workspace resolution. The root development setup
maintains these links; the prepublish guard requires removing local resolutions
before release.

## Storage and process boundaries

SQLite is canonical. Awareness defaults to
`$OCTOCODE_HOME/awareness/awareness.sqlite3`; an explicit workspace policy or
`--db-scope repo` selects `<workspace>/.octocode/awareness.sqlite3`, and an
explicit `--db` path has highest precedence for one call. Existing databases are
preserved and never merged implicitly. Agent control and Rust runtime databases remain
separate under `$OCTOCODE_HOME/agent/`. Other files and databases under
`.octocode/` retain their own owners.
The package uses Node's built-in SQLite runtime and has no npm runtime
dependencies of its own.

## Coordination flow

```text
CLI, host hook, or in-process adapter
  -> coordination command dispatcher
  -> plans/tasks/work/locks/messages/verification/memory owner
  -> shared SQLite transaction primitives
  -> the selected Awareness database
  -> compact result or explicit export
```

Awareness messages, signals, outbox entries, and verification receipts are
coordination records. They aren't agent-core lifecycle events. Native and Pi
adapters translate relevant records into their host context and acknowledge
delivery only after the owning persistence boundary succeeds. Hooks automate
declared coordination edges; they don't infer goals, claim verification, or turn
advisory presence into an exclusive lock.

`attend.next` is a structured read-first decision. Its optional command contains
literal arguments bound to the open database, workspace, artifact, and identity.
In-memory stores omit subprocess commands. Ready work is inspected before claim;
owned work is resumed without an automatic heartbeat; verification starts with an
audit and never manufactures a receipt. Host runtime guidance has no standalone
CLI actuator. The host still owns admission and authorization.

## Dependency rules

- Do not import the agent runtime, Pi, OpenTUI, or host UI policy.
- Route SQL through the module that owns the relation; do not add statements to
  CLI or presentation modules.
- Treat presence as advisory and exclusive locks as exceptional protection for
  non-mergeable work.
- Record verification only from observed checks. Memory and peer messages are
  leads, not execution proof.
- Update the canonical skill source and rebuild; do not edit generated mirrors.

See [how Awareness works](docs/HOW_IT_WORKS.md), the [database reference](docs/DB.md),
and the [documentation index](docs/README.md).
