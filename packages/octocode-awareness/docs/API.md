# Awareness API

Awareness exposes one routine operation contract through the CLI and a host-bound TypeScript client. Non-routine integration uses explicit subpaths.

## Routine operations

| Concept | Operations |
|---|---|
| Context | `context.orient` |
| Work | `work.create`, `work.list`, `work.show`, `work.claim`, `work.update`, `work.depend`, `work.protect`, `work.verify` |
| Message | `message.list`, `message.send`, `message.reply`, `message.resolve` |
| Memory | `memory.recall`, `memory.record` |
| History | `history.status`, `history.timeline`, `history.read`, `history.restore` |

The descriptor for each operation owns its validation schema, effect, optional approval class, output budget, continuation conversion, and handler. Host fields such as database, workspace, actor, session, and cancellation signal are bound by the client or CLI context instead of model parameters.

Use the live schema for exact fields:

```bash
npx @octocodeai/octocode-awareness schema commands --compact
npx @octocodeai/octocode-awareness schema command history restore --compact
```

## Package root

The root has exactly five runtime exports:

| Export | Purpose |
|---|---|
| `AWARENESS_CONCEPTS` | Ordered concept names |
| `ROUTINE_AWARENESS_OPERATIONS` | Ordered nineteen-operation inventory |
| `createAwarenessClient` | Creates a client with trusted bindings |
| `getAwarenessOperationDescriptor` | Returns one descriptor or `undefined` |
| `listAwarenessOperationDescriptors` | Returns every routine descriptor |

The root also exports the associated TypeScript types. It does not export a command dispatcher, domain repositories, SQLite helpers, hook installers, or migration internals.

## Client

Create one client per stable binding set:

```ts
import { createAwarenessClient } from '@octocodeai/octocode-awareness';

const client = createAwarenessClient({
  database: '/absolute/path/to/awareness.sqlite3',
  workspace: '/absolute/path/to/workspace',
  agentId: 'host:session-1',
  sessionId: 'session-1',
});

const orientation = await client.orient({ limit: 2 });
const result = await client.execute({
  operation: 'work.list',
  params: { kind: 'presence', limit: 2 },
});
```

`execute` returns an `AwarenessOperationResult` with `exitCode` and structured `payload`. Invalid parameters and unknown operations return `exitCode: 1`. If a serialized result exceeds its operation budget, the client returns `exitCode: 2`, `error_code: "OUTPUT_BUDGET_EXCEEDED"`, and an executable retry under `next.retry`.

Do not discard a bounded result's `partial`, `partialReasons`, omission counts, terminal limit, or executable `next` calls. Execute a continuation with the same bindings and reject repeated pages in one read chain.

## Discovery subpath

`@octocodeai/octocode-awareness/schema` exports the same four catalog and descriptor runtime values as the root, without creating a store. Tooling can inspect operation contracts without importing host or administration code.

## Host subpath

`@octocodeai/octocode-awareness/host` exposes lifecycle integration that must stay out of routine model discovery. The `createAwarenessHost` factory binds trusted context and provides `captureHistory`. The subpath also exposes the event consumer, database wake hints, host policy, prompt/context adapters, protection gate, and other typed host integration functions used by Pi.

Capture is host-owned:

```ts
import { createAwarenessHost } from '@octocodeai/octocode-awareness/host';

const host = createAwarenessHost({
  workspace: '/absolute/path/to/workspace',
  agentId: 'pi:session-1',
  sessionId: 'session-1',
});

await host.captureHistory({
  operation_id: 'operation-1',
  phase: 'before',
  file: ['src/example.ts'],
});
```

Inspect the generated type declaration before implementing a host adapter because capture fields are validated by the host contract.

## Administration subpath

`@octocodeai/octocode-awareness/admin` exports exactly three runtime functions:

- `previewDatabaseMigration`
- `applyDatabaseMigration`
- `verifyDatabaseMigration`

Migration is copy-on-write. Preview reads a recognized predecessor, classifies every row, and reports transformations without creating a destination. Apply writes a different destination file and verifies it before publication. Cutover is explicit and retains the source for rollback. For the procedure, see [Database migration](DB.md#database-migration).
