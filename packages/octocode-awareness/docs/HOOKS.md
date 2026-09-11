# Host lifecycle hooks

Hooks adapt host lifecycle events to Awareness. They are host integration, not routine model commands.

## One owner per host

Workspace policy selects exactly one lifecycle owner for each supported host:

- Pi uses native events and claims `native` ownership.
- A supported shell host can use shell hooks when policy selects `shell` ownership.
- When a host is native-owned, the shell runner exits before actor resolution, database access, or receipt writes.

Do not run native and shell lifecycle paths for the same host. Duplicate owners can duplicate presence, Message delivery, verification reminders, and History capture.

## Profiles

| Profile | Lifecycle responsibility |
|---|---|
| `coordination` | Bounded presence and changed-Message delivery at lifecycle boundaries |
| `guard` | Mutation admission, tracked file updates, and verification finalization |
| `full` | Coordination and guard behavior plus broader lifecycle History capture |

History capture remains host-owned in every profile. It is not a routine History operation.

## Admission and failure

Host payloads are untrusted. The adapter validates the event type, extracts bounded workspace-relative paths from recognized write tools, and ignores unrelated path-shaped data.

An active exclusive protection is an admission failure and blocks the conflicting write. Infrastructure failures follow the host fail-open policy and record a `degraded` hook receipt. Successful processing records `success`; a real rejected lifecycle action records `failure`.

Hook receipts provide evidence that the selected adapter ran against a store. A configuration file or static hook definition does not prove activation.

## Delivery

The host event consumer drains ordered SQLite events serially. It acknowledges an event only after the host has persisted or accepted the delivery. On failure, the event remains available for a later wake.

Database and WAL watchers are read-only wake hints. They coalesce changes and prompt another authoritative drain; they do not carry Message bodies. Lifecycle drains remain available when a watch is unavailable.

Message delivery, actor-specific Message reads, and thread resolution are independent facts.

## Integration boundary

Host code imports `@octocodeai/octocode-awareness/host`. It must not launch the Awareness CLI or copy its schemas, policy, SQL, event cursor, or History writer.

Installation, removal, and host configuration mutation require their own scoped authorization outside the routine Awareness surface. After changing an adapter, verify both the package contract and one real lifecycle receipt in the target host.

For workspace ownership fields, see [Configuration](CONFIGURATION.md). For capture semantics, see [LocalGit history](LOCAL_HISTORY.md).
