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

The descriptor for each operation owns its validation schema, exact compact `inputSchemaText`, effect, optional approval class, output budget, continuation conversion, and handler. The text is serialized from the descriptor schema, not maintained separately. Host fields such as database, workspace, actor, session, and cancellation signal are bound by the client or CLI context instead of model parameters.

Use the live schema for exact fields:

```bash
npx @octocodeai/octocode-awareness schema commands --compact
npx @octocodeai/octocode-awareness schema command history restore --compact
```

Native hosts read `descriptor.inputSchemaText`; Pi returns that same value from `describe:true`. The complete external-host guide includes it for every operation, while standing instructions keep schemas on demand to avoid paying their token cost on every turn.

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

Use `kind: 'agents'` to inspect visible identities directly. This read-only projection unions registered identities with distinct senders and addressed recipients observed in Message rows, labels each row `registered` or `observed`, applies the bound workspace's linked-checkout scope, and includes store-global registrations. It does not implicitly filter to the calling agent or register an actor merely because `context.orient` was called. Results are stably ordered and paged after deduplication; when `partial` is true, execute `next.list` to advance `offset` without gaps or duplicates.

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

## Operator HTML view

`view` is an operator-only CLI command. It creates a private, self-contained HTML snapshot and opens it in the platform browser by default:

```bash
npx @octocodeai/octocode-awareness view --workspace "$PWD"
npx @octocodeai/octocode-awareness view --workspace "$PWD" --out .octocode/awareness.html --no-open
```

The page includes the workspace-scoped agent projection, every row and column from every canonical SQLite entity, each entity's canonical owner, lifecycle policy, and executable cleanup operation, and the workspace-scoped `history.status` projection for LocalGit. Its top cards label scope explicitly: agent and LocalGit values are workspace-scoped, while entity rows and the SQLite row count are store-wide. The Agents panel uses the same registered-plus-observed projection as `work.list` with `kind: 'agents'`. LocalGit file bytes are deliberately not copied into the page; the history ledger still exposes capture paths, object IDs, durability, restores, and operations. The generated file uses mode `0600`, contains no remote assets, and escapes stored text before rendering. Re-run `view` to refresh the snapshot.

`view` stays outside `schema commands` because it is an operator inspection surface, not a routine agent operation. Use `view --help` for its exact flags. `--db`, `--db-scope`, and `--workspace` select the same store and workspace bindings used by routine commands.

## Operator lifecycle commands

Operator commands are intentionally absent from routine agent discovery. Their canonical strict Zod contracts are still inspectable and executable as JSON Schema:

```bash
npx @octocodeai/octocode-awareness schema command maintenance retention --compact
npx @octocodeai/octocode-awareness schema command maintenance store-retire --compact
```

`maintenance retention` reports by default. Confirmed apply uses bounded per-owner batches and returns an executable `next` command with the original cutoff until the eligible set is empty. It expires Messages, preserves their grace window and thread ancestry, retires old interactions/events/runs/Memories/restore previews, and removes expired locks. Abandoned ACTIVE runs are reported but are failed only when `--fail-stale-active-runs` is supplied; each failure receives a verification event.

`maintenance store-retire` reports exact store targets and blockers by default. Save and inspect that JSON. `--action apply --confirm retire --report-file <reviewed-json>` revalidates the bound report, refuses active lifecycle state and SQLite writers, and quarantines exact SQLite and LocalGit targets by sibling rename. The result contains recovery paths; it does not permanently erase them.

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

`@octocodeai/octocode-awareness/admin` exports migration and store-retirement services:

- `previewDatabaseMigration`
- `applyDatabaseMigration`
- `verifyDatabaseMigration`
- `reportStoreRetirement`
- `applyStoreRetirement`
- `StoreRetirementError`

Migration is copy-on-write. Preview reads a recognized predecessor, classifies every row, and reports transformations without creating a destination. Apply writes a different destination file and verifies it before publication. Cutover is explicit and retains the source for rollback. For the procedure, see [Database migration](DB.md#database-migration).

Store retirement is also report-bound and recoverable. It quarantines exact files and directories only after lifecycle and writer checks; it is not exported from the routine package root.
