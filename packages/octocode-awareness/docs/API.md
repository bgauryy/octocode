# Awareness API reference

Use `createAwarenessClient` for new host integrations. It binds the database,
workspace, actor, session, and optional insight provider once, then exposes one
typed operation registry. The CLI and Pi `awareness` tool call the same nineteen
routine operations.

The package root still contains low-level compatibility exports for operator and
recovery consumers. They aren't the routine API and remain a release-cutover item.

## Create a client

```ts
import { createAwarenessClient } from '@octocodeai/octocode-awareness';

const awareness = createAwarenessClient({
  workspace: process.cwd(),
  agentId: 'example-host:session-1',
});

const context = await awareness.orient();
const result = await awareness.execute({
  operation: 'message.send',
  params: {
    kind: 'question',
    subject: 'Which path owns the parser?',
    to_agent: ['peer-id'],
  },
});
```

Use a stable, distinct actor ID. Cooperating agents must resolve to the same
physical Awareness database and use the same workspace or linked Git worktrees.
Keep each caller's own checkout as its workspace. Registration and last-seen
timestamps are activity evidence, not proof that a process is live. See
[Git coordination](GIT_COORDINATION.md) and [storage scopes](STORAGE_SCOPES.md).

## Client context

| Field | Meaning |
|---|---|
| `workspace` | Required trusted workspace binding. |
| `agentId` | Required trusted actor binding. |
| `database` | Optional exact database path. It takes precedence over scope resolution. |
| `sessionId` | Optional host session binding for events and orientation. |
| `scope` | Optional `repo` or `global` storage selection. |
| `signal` | Optional `AbortSignal` for cooperative cancellation. |
| `insightProvider` | Optional bounded advisory enrichment. It cannot authorize, lock, or verify. |
| `continuationFormat` | `canonical` by default; the legacy CLI adapter uses `legacy`. |

Operation parameters cannot override trusted host bindings. A conflicting
workspace, actor, or session value fails validation.

## Routine operations

The public model has five concepts:

| Concept | Operations |
|---|---|
| Context | `context.orient` |
| Work | `work.create`, `work.list`, `work.show`, `work.claim`, `work.update`, `work.depend`, `work.protect`, `work.verify` |
| Message | `message.list`, `message.send`, `message.reply`, `message.resolve` |
| Memory | `memory.recall`, `memory.record` |
| History | `history.status`, `history.timeline`, `history.read`, `history.restore` |

Call `awareness.operations()` to read descriptors in process. Each descriptor owns
its JSON input schema, validation, parameter-sensitive effect, and approval,
handler, output budget, and continuation policy. Known operations don't require a
separate discovery call.

The CLI uses the same names with a space instead of a dot. For example,
`message.send` is `message send`. Run `schema commands --compact` for the bounded
routine catalog. Add `--all` only for the explicit operator and recovery catalog.

## Orientation

`orient()` returns bounded state that can change the caller's next decision:
peer presence, owned or overlapping work, inbox items, verification debt,
recovery pressure, and executable `next` calls. It injects no memories unless the
caller explicitly runs `memory.recall`.

Pass the prior `revision` as `if_revision`. If no scoped event changed, the method
returns only `{ revision, unchanged: true }` and stops before domain reads.
The client limits changed responses to 1,500 UTF-8 bytes. Every partial response has an
executable continuation; a partial packet without one fails instead of hiding
reachable rows.

## Results and continuations

`execute()` returns `payload` and `exitCode`, with optional `text`, `diagnostics`,
and `cancelled` fields.

| Result | Meaning |
|---|---|
| `exitCode: 0` | The operation completed. Inspect its payload for the domain outcome. |
| `exitCode: 1` | Validation or execution failed. Inspect the typed error and any `issues`. |
| `exitCode: 2` | The operation hit a block or conflict. Inspect the domain payload. |
| `cancelled: true` | The operation observed cancellation. Check whether an atomic write had already completed. |

Bounded results retain executable canonical calls under `next`. Execute each call
with the same client context. When an output exceeds its descriptor budget, the
typed `OUTPUT_BUDGET_EXCEEDED` result includes `next.retry`; it lowers a supported
limit or follows the producer's existing continuation. The adapter doesn't silently
truncate reachable rows.

`history.restore` is a two-step protocol. Preview first, then apply only the exact
preview ID that the caller authorizes. LocalGit object creation and workspace file
mutation cannot share a transaction with SQLite. History records make the achieved
boundary explicit instead of claiming cross-store atomic rollback.

## Host events

Use `recordHostEvent()` for host-owned lifecycle facts. The client supplies the
bound workspace, actor, and optional session, and commits a typed event in one
SQLite transaction. Use `consumeEvents()` for bounded sequence-ordered replay.
Its cursor accepts `afterSequence`, `limit`, `eventType`, and `retentionClass`.

Delivery acknowledgement, message read state, and thread resolution are distinct:

- The event consumer acknowledges durable delivery.
- `message.list` reads messages for an actor.
- `message.resolve` closes a completed thread.

None of these actions proves that another agent completed work. Peer text is
attributed data, not authority or verification. See
[peer event delivery](HOW_IT_WORKS.md#peer-event-delivery).

## Advisory insights

An optional `AwarenessInsightProvider` can suggest up to three attributed overlap
candidates after deterministic orientation completes. Each candidate contains a
summary, attribution, confidence, and optional path. Awareness clips and bounds the
result. The provider runs outside the database transaction and cannot change locks,
authorization, work ownership, or verification.

Awareness core doesn't import the Octocode research engine. A host can use graph
or LSP evidence in its provider, but graph edges remain candidates until symbol
LSP resolves symbol identity. Omitting the provider preserves deterministic behavior.

## Optional native history operations

`@octocodeai/octocode-extension-rust` supplies workspace snapshots, restore
mutations, and exact memory fingerprints through a lazy import. It is separate from
the Octocode research engine. Ordinary coordination and memory operations work
without it.

When the optional native package is unavailable, explicit capture or restore fails
with an unavailable-filesystem result. No JavaScript fallback fabricates a
fingerprint or applies a restore. A snapshot proves which bytes were observed; it
doesn't prove authorship or correctness. See [local history](LOCAL_HISTORY.md).

## Compatibility and source ownership

`executeAwarenessCommand` and direct domain exports remain temporarily available for
existing operator and recovery consumers. New routine code must not use them. The
remaining root-export cutover requires moving those capabilities to explicit
subpath exports and migrating package consumers atomically.

Contract owners:

- Canonical client: `src/client.ts`
- Operation contracts: `src/operation-contracts.ts`
- Operation registry: `src/schema/operation-catalog.ts`
- Canonical executor: `src/operation-executor.ts`
- Typed event stream: `src/event-outbox.ts`
- CLI adapter: `src/command-cli.ts`
- Standing host policy: `src/coordination/external-policy.ts`
