# Awareness architecture

`@octocodeai/octocode-awareness` owns durable coordination, memory, hooks,
reflection, and recovery for coding agents. It remains independent of
`@octocodeai/agent-core`; hosts translate Awareness facts through adapters.

The default CLI `attend` route uses `src/attend-presence.ts`: one bounded registry query with executable peer-page continuations. `--details` or query/file filters select the detailed `attend-query.ts` observer. Default hooks use `peer-briefing.ts` for messages only; no memory retrieval or per-edit work records. Guard/full profiles opt into mutation bookkeeping, while global verification and session-capture features default off.

## Ownership

- Root domain modules own plans, tasks, work presence, locks, signals, memory,
  agents, and verification. `src/coordination/` adapts those owners for hosts
  and owns handoffs and continuity; it has no separate lifecycle or store.
- `src/db-runtime.ts`, `src/db-schema.ts`, `src/sql/`, and schema modules own
  SQLite opening, identity, statements, and maintenance. Explicit database
  conversion writes a new store. Opening recognizes one exact predecessor
  fingerprint to add the capture-durability ledger; existing captures retain
  unknown durability rather than receiving an inferred successful flush.
- `src/attend-*`, signals, refinements, sessions, query, digest, reflection, and
  maintenance modules own the advanced operating and learning workflows.
- `src/command-api.ts` exposes the command catalog as structured requests. `command-dispatch.ts` routes to `src/commands/` handlers and `src/hooks/` owns reusable host callback behavior. Request-local output keeps simultaneous callers isolated; handlers throw errors and never terminate the process. `command-cli.ts` owns shell parsing and environment defaults; `bin/awareness.ts` renders the result and owns process exit. The hook entry adapter similarly owns argv/stdin. The native executor imports neither entrypoint. Native hosts never launch or parse the Awareness CLI.
- The package-local `skills/octocode-awareness/` directory is the bundled skill
  source. `@octocodeai/octocode-skill-installer` owns platform paths, atomic
  materialization under `$OCTOCODE_HOME/skills`, conflict handling, and links from
  host directories to that durable copy. Generated helpers, `out/skills/`, and
  `.agents/skills/` are build output.
- `@octocodeai/agent-contracts` owns Agent control-database paths and tables,
  low-level SQLite utilities, shared entity types, and cross-host protocol
  fragments. Shared types and utilities don't imply shared physical storage.

During root development, `@octocodeai/agent-contracts` resolves to the local workspace
`packages/octocode-agent-contracts`. Rebuild that owning workspace before building
Awareness or Pi consumers; no sibling snapshot or dependency reinstall is needed
for source changes. The published Awareness package has zero mandatory npm runtime dependencies;
its build bundles the required shared contracts. File fingerprints and workspace
history capture/restore use the optional `@octocodeai/octocode-extension-rust`
package, loaded through `src/native-files.ts`. That package owns the native
filesystem boundary separately from the `@octocodeai/octocode-engine` research
engine; Awareness does not import the research engine. Native hosts consume Awareness's
public package API, while Pi uses the local Awareness workspace during development. The native tool, checkpoints, history hooks and optional status scheduler all call this API directly. Lock waits yield to the event loop and support cooperative cancellation. History implementation loads only when requested.

## Compact policy and observations

`EXTERNAL_AGENT_AWARENESS_PROMPT` is the canonical standing cooperation policy.
CLI `instructions export`, Pi and the bundled skill route agents to it; the full
`EXTERNAL_AGENT_AWARENESS_INSTRUCTIONS` remains an explicit `guide` reference.
The capability summary derives from the command registry. Hosts add runtime
bindings, not another copy of the shared policy.

`src/attend-revision.ts` compares fresh scoped observations without a second cache.
`src/memory-evidence.ts` validates explicit source/dependency bytes through existing
memory references and fingerprints asynchronously, outside write transactions.
`memory-write.ts` separates async capture from synchronous insertion and the
atomic similarity gate; synchronous internal queries cannot request fingerprints.
`getMemory`, `insertMemory`, `recallMemory`, and canonical client operations that
capture or validate evidence return promises. Evidence checks share one 100 ms filesystem budget across returned rows,
including canonicalization and native worker queue time. First-use native module
initialization precedes that budget and still contributes to total request latency. Neither observation
creates authorization or successful verification. See [navigation](docs/MEMORY_NAVIGATION.md)
and [evidence reuse](skills/octocode-awareness/references/memory-recall.md).

## Evidence boundaries

Keep four claims separate when evaluating a host or workflow:

1. **Implemented contract**: package code, schemas, and tests define the behavior.
2. **Configured surface**: settings or generated definitions contain the integration.
3. **Activated behavior**: the running host loaded and invoked that surface.
4. **Observed receipt**: the selected Awareness store contains the resulting event or
   verification evidence.

A contract test can establish the first level and inspect the second, but it cannot
prove activation or a live receipt for every vendor. Likewise, a process-scoped
persistent policy can keep native worker state available to one harness run without
proving cross-process or cross-machine delivery. See the [harness boundary](docs/HARNESS.md)
and the [host support matrix](docs/HOOKS.md) for the evidence required at each level.

## Storage and process boundaries

SQLite is canonical. Awareness defaults to
`$OCTOCODE_HOME/awareness/awareness.sqlite3`; an explicit workspace policy or
`--db-scope repo` selects `<workspace>/.octocode/awareness.sqlite3`, and an
explicit `--db` path has highest precedence for one call. Existing databases are
preserved and never merged implicitly. Agent control and Rust runtime databases remain
separate under `$OCTOCODE_HOME/agent/`. Other files and databases under
`.octocode/` retain their own owners.
The package uses Node's built-in SQLite runtime. The optional native file package
is required only by operations that inspect or restore workspace file bytes.

Local history keeps metadata in canonical Awareness SQLite tables and raw file
objects under `<workspace>/.octocode/.localGit`, partitioned by canonical database
path and workspace identity. `history-store.ts` owns placement; `history status`
reports the exact directory and any old sidecar requiring offline relocation.
The bundled `isomorphic-git` backend operates only on that private bare store: it
does not read or write the workspace Git index, refs, configuration, hooks, remotes,
or objects, and it needs no system Git or network. Sidecars are lazy and are never
created for status checks or in-memory stores. Database consolidation rejects
history-bearing sources until an explicit sidecar copy and integrity protocol is
implemented. See [local file history](docs/LOCAL_HISTORY.md).

## Coordination flow

Git owns linked-worktree membership; peer discovery, signals and memory share
that read scope within one database. Physical workspace keys remain unchanged
for locks, recovery, verification and authorization. See
[local Git coordination](docs/GIT_COORDINATION.md) for the decision and usage.

```text
CLI, host hook, or in-process adapter
  -> executeAwarenessCommand({ command, params }, context)
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

Default `attend` returns peer pages. The detailed observer's `attend.next` is a
structured read-first decision. CLI output uses literal argument arrays; the
[command API](docs/API.md) returns request objects executed with the same trusted context.
In-memory stores omit subprocess commands. Ready work is inspected before claim;
owned work is resumed without an automatic heartbeat; verification starts with an
audit and never manufactures a receipt. Host runtime guidance has no standalone
CLI actuator. The host still owns admission and authorization.

## Dependency rules

### Delivery and retry ownership

`createAwarenessEventConsumer` owns ordered, serialized drains, inbound policy,
acknowledgements and retryable delivery state. Hosts consume its public API and
`createAwarenessEventObservability` defaults; they do not duplicate the outbox or
its counters. A failed delivery remains unacknowledged and reports error pressure.
The host supplies lifecycle or database-hint wakes and proves persistence before accepting
delivery. Delivery acknowledgement, signal handling and thread resolution are
separate operations.

Each asynchronous consumer drain validates Git membership once through
`withRepositoryWorkspaceScope`, before opening the store. Its callback scope
reuses the complete physical membership for synchronous queries and receipts;
no Git subprocess runs per message or inside acknowledgement transactions.
The scope expires on callback completion and the next drain rediscovers removed
worktrees. Synchronous callers retain the uncached discovery path.

`watchAwarenessEventHints` watches the selected database directory, database file
and WAL. Its coalesced, read-only maximum-outbox-sequence check ignores receipt
and reader churn. Directory events reattach file watches after WAL recreation.
Watch/read failures retry after 100 ms, 500 ms, and 2 seconds, then remain armed
on a 30-second recovery interval until success; healthy idle sessions do not poll.
Hints contain no message bodies and never substitute for an authoritative drain.

| Boundary | Retry owner and bound |
|---|---|
| SQLite busy state | Shared `agent-contracts` utility; 25 ms delay and 10 s deadline for wrapped operations, plus SQLite's configured busy timeout. |
| Explicit lock wait | Shared command API for native and CLI calls; bounded wait, cooperative cancellation, and lease expiry. Lower-level single-file APIs remain available. |
| Failed task | Explicit lead-owned task retry; no automatic model retry. |
| Peer delivery | One in-flight drain, 100 events by default; failed delivery stops the drain and waits for another host wake. |
| Private Git refs | Atomic create-only publication of complete immutable refs; competing writers cannot overwrite a winner. No expiring filesystem lease is involved. |

These waits serve different contracts. Combining them into a generic retry loop
would lose transaction, lease or delivery semantics. Latency across nested
boundaries still needs measurement; a per-operation deadline is not a total
workflow deadline.

### Layer rules

- Do not import the agent runtime, Pi, OpenTUI, or host UI policy.
- Keep command handlers independent of `bin/` and shell identity/output defaults. Native bindings and options belong to each request.
- `src/schema/registry.ts` owns the Zod schema inventory; `command-input.ts` projects command fields once for validation, discovery and host binding metadata. Command descriptions own selection guidance; the standing prompt owns cross-command behavior.
- Route SQL through the module that owns the relation; do not add statements to
  CLI or presentation modules.
- Treat presence as advisory and exclusive locks as exceptional protection for
  non-mergeable work.
- Record verification only from observed checks. Memory and peer messages are
  leads, not execution proof.
- Update the canonical skill source and rebuild; do not edit generated mirrors.

See [how Awareness works](docs/HOW_IT_WORKS.md), the [database reference](docs/DB.md),
and the [documentation index](docs/README.md).
