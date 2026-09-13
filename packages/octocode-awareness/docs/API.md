# Awareness API

Awareness exposes one routine operation contract through the CLI and a host-bound TypeScript client. Non-routine integration uses explicit subpaths.

## Routine operations

| Concept | Operations |
|---|---|
| Context | `context.orient`, `context.observe`, `context.feedback` |
| Work | `work.create`, `work.list`, `work.show`, `work.claim`, `work.update`, `work.depend`, `work.protect`, `work.verify` |
| Message | `message.list`, `message.send`, `message.reply`, `message.resolve` |
| Memory | `memory.recall`, `memory.record`, `memory.set`, `memory.get`, `memory.revalidate` |
| History | `history.status`, `history.timeline`, `history.read`, `history.restore`, `history.experience` |

The descriptor for each operation owns its validation schema, effect, optional approval class, output budget, continuation conversion, and handler. Host fields such as database, workspace, actor, session, and cancellation signal are bound by the client or CLI context instead of model parameters.

Use the live schema for exact fields:

```bash
npx @octocodeai/octocode-awareness schema commands --compact
npx @octocodeai/octocode-awareness schema command history restore --compact
```

## Package root

The root has exactly eight runtime exports:

| Export | Purpose |
|---|---|
| `AWARENESS_AGENT_INSTRUCTION_SECTIONS` | Available reusable agent instruction sections |
| `AWARENESS_CONCEPTS` | Ordered concept names |
| `AWARENESS_MESSAGE_PARAMETER_GUIDANCE` | Concise canonical Message field guidance for host prompts |
| `ROUTINE_AWARENESS_OPERATIONS` | Ordered routine operation inventory |
| `createAwarenessClient` | Creates a client with trusted bindings |
| `getAwarenessAgentInstructions` | Builds canonical agent guidance for host prompts |
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

## Observations and feedback

`context.observe` records attributed measurements for the bound actor and session. `context.orient` projects available measurements, workspace signals, and advisory regulation. `context.feedback` records the response to advice and links outcome evidence. All three are callable through `execute` or the CLI; no particular agent runtime is required.

Discover their exact input fields with `schema command context observe` and `schema command context feedback`. Supply measured values only. Missing or expired sensors remain unavailable and do not imply degraded recovery. An action report alone does not prove improvement. Observations and feedback use the existing event outbox without a database schema change.

Import `getAwarenessAgentInstructions` to obtain package-owned operating guidance for a host prompt. `AWARENESS_AGENT_INSTRUCTION_SECTIONS` lists the available sections. Hosts that need only the failure-sensitive Message field summary can reuse `AWARENESS_MESSAGE_PARAMETER_GUIDANCE`. This keeps agent guidance aligned with the operation catalog instead of requiring each host to maintain its own explanation.

```ts
import { getAwarenessAgentInstructions } from '@octocodeai/octocode-awareness';

const instructions = getAwarenessAgentInstructions({
  sections: ['start', 'observe', 'advise', 'feedback', 'trust'],
});
```

The CLI equivalent is `instructions`; repeat `--section <name>` to compose selected sections. Omitting sections returns all guidance. The instruction builder does not open a store.

## Discovery subpath

Keyed lessons, applicability checks, and non-file investigation traces use the same client and bindings. See [Experience and anchored memory](EXPERIENCE_MEMORY.md) for their lifecycle, examples, and limits.

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
