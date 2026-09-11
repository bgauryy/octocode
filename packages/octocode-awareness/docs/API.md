# Awareness API reference

The package root exports the complete command API, command descriptors, and canonical agent instructions. Native hosts import these exports directly. The CLI parses shell input and renders results from the same executor. Reusable handlers live under `src/commands/`; `bin/` contains shell adapters and process entrypoints.

## Execute a command

```ts
import {
  executeAwarenessCommand,
  type AwarenessCommandContext,
} from '@octocodeai/octocode-awareness';

const context: AwarenessCommandContext = {
  workspace: process.cwd(),
  agentId: 'example-host:session-1',
  compact: true,
};

const result = await executeAwarenessCommand(
  { command: 'attend', params: { limit: 5 } },
  context,
);
```

Use a stable, distinct session identity in a real host. Cooperating agents need the same physical database and workspace, or linked Git worktrees. Keep each caller's own checkout as workspace; see [Git coordination](GIT_COORDINATION.md). `attend` reads the registry; native hosts register and leave through their lifecycle integration. Registration and last-seen timestamps are activity evidence, not proof that a process is live.

`AwarenessCommandCall` contains a canonical command name and optional `params`. Parameter names match descriptor fields, such as `to_agent` and `include_bodies`; they are not CLI flags. Discover an unfamiliar command with `getAwarenessCommandDescriptor(command)`, or enumerate with `listAwarenessCommandDescriptors()`. Descriptors own required fields, limits, approval classes, and host exposure.

Use `{ command: 'attend', params: { changes: true, limit: 10 } }` when Git changes
or peer work affect the next action. The opt-in view returns separate `git` and
`work` rows across linked checkouts. Follow `next.list.command` for another page
or a work row's `next.inspect.command` for full rationale and test plan. Reuse the
same context; the continuation retains the caller's workspace. The status revision
detects changes to the listed paths, status or declarations, not equality of file
bytes. A changed page returns `partialReasons: ['snapshot_changed']` and a restart.

## Execution context

| Field | Meaning |
|---|---|
| `database` | Exact Awareness database path. Takes precedence over scope-based resolution. |
| `workspace` | Workspace binding supplied by the host. |
| `agentId` | Participant binding supplied by the host. |
| `scope` | Optional `repo` or `global` storage selection. When omitted, command-family workspace policy applies. |
| `compact` | Selects compact output; defaults to `false`. |
| `signal` | Optional `AbortSignal` for cooperative cancellation. |
| `continuations` | `api` by default; `cli` retains shell continuation metadata for CLI adapters. |
| `readInput` | CLI adapter callback for resolving schema-validation input from a file or stdin. Native callers omit it. |

Keep trusted host bindings separate from model-provided parameters. Conflicting workspace or identity overrides are rejected. Native calls do not inherit CLI agent labels or compact-output environment defaults; pass labels in registration parameters and choose `context.compact` explicitly. The shell adapter alone applies those environment defaults. Preserve the same context when following continuations; a command object does not replace the database binding. See [storage scopes](STORAGE_SCOPES.md).

For `work list` and `work show`, optional `params.agent_id` selects whose work to
read. It is not an actor override and is not filled from `context.agentId`. Omit it
to inspect peer overlap; mutations continue to enforce the host's actor binding.

`memory recall-verified` accepts either a search query or an exact `memory_id`; the
exact form cannot be combined with `query`. Source digest, scope, and expiry checks
still apply to an exact pointer.
Reads include existing linked Git worktrees in the same physical repository;
`strict_scope: true` restricts them to the opened workspace. Unrelated clones are
excluded. File filters are conjunctive paths relative to each source workspace,
including when the caller supplies an absolute path inside its own workspace.
Every result returns its source `workspacePath`; returned `file` paths are
relative to that source. These read scopes do not change write or restore bindings.

Verified-memory reads return a page: `memories`, `partial`, `partialReasons`, and
an executable `next.call` when another page is available. Consumers must read
`payload.memories`; the response is no longer a bare array. Keep the returned
filters and observation time when following the call. Bounded semantic ranking
must expose its terminal limit rather than imply exhaustive evidence coverage.
If a peer changes the result set between pages, `snapshot_changed` returns a
restart call. Discard the old page chain before restarting; do not combine it
with the new revision. Revisions also bind the caller workspace, Git membership,
query and filters; execute each continuation with its original host context.

Use verified memory for selected file reasoning: concise `text`, a `why` or
`constraint`, source digest and validity, optional file/area, and a real `artifact`
identifier for artifact scope. An optional `history_ref` links existing immutable
evidence; storing a note does not capture file bytes. Exact duplicate stores reuse
their record, while explicit supersession replaces obsolete knowledge.
`historyEvidence` distinguishes `recorded` capture metadata from `incomplete`
or `unavailable` evidence. It does not assert present-day file freshness or byte
availability. Follow `historyEvidence.next.call` to inspect the operation in its
`source_workspace`, then use the returned read calls for selected captured bytes.
Discover
the current field schema before constructing an unfamiliar request.
The complete stored evidence packet has an 8192-byte budget. Keep one reusable
reason in a record and link detailed evidence instead of copying a file or dialogue.
Verified recall also limits its memory array to 16 KiB; the executable continuation
advances by the records actually returned, even when fewer than `limit` fit.

## Results and continuations

`AwarenessCommandResult` returns `payload` and `exitCode`, with optional `text`, `diagnostics`, and `cancelled`. Read the command's payload as well as its exit code:

| Result | Interpretation |
|---|---|
| `exitCode: 0` | Command completed successfully. Some reference/export commands return `text`. |
| `exitCode: 1`, `payload.ok: false` | Invalid input or execution failure; inspect the error. |
| `verify audit` with `exitCode: 1`, `payload.ok: true` | Audit completed and found debt needing attention. It is not an executor failure. |
| `exitCode: 2` | Conflict or blocked operation; inspect the command-specific payload. |
| `cancelled: true` | Cooperative cancellation was observed; inspect the reported outcome before retrying. |

Bounded results expose partial state and an executable continuation where another page or read is available. API continuations contain `{ command, params }` requests instead of shell argument arrays. For default presence attendance, the next request is `payload.next.list.command`. Execute it with the same trusted context. Other routes can place requests under their own `next` fields. A terminal-limit diagnostic means the result cannot be extended by that route; do not treat the bounded packet as complete.

History API continuations use `next.call: { command, params }`; the CLI adapter keeps its `next.argv` form. Follow the returned call with the same database and workspace context.

Signal lists expose a page-level acknowledgement action with exact IDs; explicit
signal reads can expose reply drafts. API actions use `{ command, params }`; CLI
actions supply argv. Fill a draft with a substantive answer before executing it.
These are suggestions, not automatically performed mutations. Restore apply
returns `undo_preview`, targeting the durable checkpoint's available after image;
execute that preview request before deciding whether to authorize another apply.

Detailed attendance is selected by `details`, `query`, `file`, `artifact`, `repo`,
`ref`, `include_bodies`, `explain_organ`, or `revision`; `changes` selects the
separate Git view and cannot combine with those detail filters except
`include_bodies` and `revision`. An unchanged response preserves its `revision`,
`workspace_path`, partial/omission fields, `next` action, and executable
continuations; retain the previous packet and follow those continuations for
omitted detail. Equality is reported only when the bounded comparison snapshot is
complete. Workboard and memory safety caps remain explicit and prevent an
incomplete snapshot from being reported unchanged.

The API never launches the Awareness binary, captures process stdout, changes cwd or environment, reads stdin, or exits the host. Output is request-local across asynchronous calls. Lock waits yield to the event loop and honor cancellation. Cancellation cannot preempt every synchronous operation; completed atomic writes are reported as completed.

`schema validate` accepts a JSON value or serialized JSON in API `params.input`. The CLI accepts a JSON file path or `-` for stdin. Native callers do not need a file adapter.

## Agent instructions

`EXTERNAL_AGENT_AWARENESS_PROMPT` owns the short standing policy. `AWARENESS_PI_HOST_PROMPT` is an alias of the same text. Add it once to system instructions; add host bindings separately. CLI `instructions export` returns the same policy.

The full `EXTERNAL_AGENT_AWARENESS_INSTRUCTIONS` and `getExternalAgentAwarenessGuide()` are on-demand references, also available through `guide`. Avoid placing the complete guide or command catalog in every turn. The policy starts with one peer briefing, useful communication, and conditional feature discovery. Store learning only when a verified reason or constraint changes a future decision.

The standing policy directs agents to the tracked-work recipe before using work,
task, or verification features. Exact close/submit, verification, and audit procedures
live in that recipe and the full guide. Native integrations retain their existing
task/run IDs and observed receipts. Routine attendance and message handling do not
require tracking records or an audit ritual.

## Optional native file operations

Awareness has zero mandatory npm runtime dependencies. `@octocodeai/octocode-extension-rust`
is an optional dependency with a matching platform addon. It supplies workspace
snapshots, restore mutations, and exact memory fingerprints through a lazy import;
it is separate from the Octocode research engine. Library imports, ordinary CLI
commands, memory writes without capture, and unchecked recall work without it.

Package-manager installs with optional dependencies enabled make the addon available
to the package CLI and its bundled skills. A copied standalone skill resolves the
addon from its own directory and ancestor `node_modules` directories. For a project
skill under `.agents/skills`, install `@octocodeai/octocode-extension-rust` as an
optional dependency in that project, or use the package CLI. A global Awareness
installation does not supply dependencies to a skill copied into an unrelated
project. Skill installation copies runtime assets; it does not install npm packages.

When the addon is absent, explicit history capture/restore fails with an unavailable
native-filesystem error. Fingerprint capture rejects with `source_inaccessible`
and inserts no memory. Checked recall remains available and reports evidence as
`unknown` when native validation cannot run. No JavaScript filesystem fallback
creates a fingerprint or applies a restore.

The lower-level `insertMemory`, `insertMemoryWithSimilarityGate`, `getMemory`, and
`recallMemory` APIs return promises; await their
results before using or closing the database. Capture completes before a SQLite
write transaction. Internal synchronous write/query phases reject requests for
unprepared capture or filesystem checking.

Memory evidence retains the `awareness-evidence-v1:` fingerprint format. One
filesystem budget covers all returned rows: 64 references, 1 MiB per file,
8 MiB in total, and 100 ms including canonicalization and native worker queue time.
First-use native module initialization happens before the filesystem deadline;
it adds to total request latency. A limit or unstable source never produces a
validated fingerprint. See [local history](LOCAL_HISTORY.md) for its separate
capture limits and restore contract.

## Host callbacks and message delivery

All commands are library-callable. Pi's native `awareness` tool excludes descriptors marked `external-host-only`: `hook run` and `hooks pre-edit` are internal host callbacks. Setup routes such as `hooks install`, `skill install`, and `instructions export` remain available through Pi; mutating setup retains its approval requirements.

`hook run` requires an explicit payload and resolves storage from that payload's workspace. It rejects context `database` or `scope` overrides. `hooks pre-edit` accepts an object or serialized JSON in `params.event_json` and uses the supplied host bindings.

Hosts can also import `openAwarenessStore`, `createAwarenessEventConsumer`, and the lower-level domain APIs exported by [the package root](../src/index.ts). The older `execCli` and `dispatchAwarenessCommand` helpers cover only a subset of routes; use `executeAwarenessCommand` for the full catalog.

Event consumers drain at host lifecycle opportunities. The host supplies wake-ups and persists accepted messages before acknowledging delivery. Native `sendMessage` accepts optional `data`; native inbox messages expose human `text` and validated `data` separately. Accepted deliveries retain it in `details.data` and attributed content. Hooks emit `has_data` with an executable full-read continuation instead of clipping machine JSON. Delivery acknowledgement, signal handling (`signal ack`), and thread completion (`signal resolve` with `thread_id`) are separate. There is no package background message watcher. See [peer event delivery](HOW_IT_WORKS.md#peer-event-delivery) and [Pi integration](../../octocode-pi-extension/docs/AWARENESS_AGENT_FLOW.md).

Command schemas and host-injected field metadata derive from the same projected Zod fields, including union branches. CLI help exposes those fields directly.

`signal publish` and `signal reply` may carry structured `data` as an object or JSON
string using the versioned `signal/v1` envelope with caller-defined `{type, payload}`
alongside the human body. The envelope is bounded to 4000 UTF-8 bytes.
`signal list --include-bodies` exposes `data`; compact or summarized rows omit its
payload and retain `has_data: true`.
`encodeSignalBody` and `decodeSignalBody` support adapters that explicitly handle
the stored wire envelope; ordinary command and native inbox callers use `data`.

Source owners: [command API](../src/command-api.ts), [command schemas](../src/schema/cli.ts), [continuations](../src/command-continuations.ts), and [standing policy](../src/coordination/external-policy.ts).
