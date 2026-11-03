# CLI and MCP entry points

Use the default npm entry for a host MCP connection. Use `/cli` for standalone operations.
Both routes execute the same in-process TypeScript runtime and SQLite contract.
The data model borrows A2A terminology for message parts, correlated requests and artifacts. Requests remain messages and named documents provide artifact storage; there is no separate Task or Artifact API, remote endpoint, or A2A wire compatibility.

```mermaid
flowchart LR
  N[npm executable] --> M[MCP stdio]
  N --> C[/cli operations]
  M --> P[TypeScript runtime]
  C --> G[octocode-mcp-cli validation and help]
  G --> P
  P --> S[(Shared SQLite store)]
```

## Discover before operating

```sh
npx -y @octocodeai/octocode-agents-communication --help
npx -y @octocodeai/octocode-agents-communication /cli --help
npx -y @octocodeai/octocode-agents-communication /cli send_message --help
npx -y @octocodeai/octocode-agents-communication /cli send_message --help --json
npx -y @octocodeai/octocode-agents-communication /cli schema send_message --json
npx -y @octocodeai/octocode-agents-communication /cli schema types --compact --json
```

Command help describes CLI flags, including identity and workspace controls; `schema` describes the operation input without them. The CLI exposes five capability commands, the low-level lifecycle/host/store operations, and two record-type discovery routes (`schema types`, `schema type`), which take no workspace, database or session flags. Bound MCP exposes five typed capability tools, and `--tools` profiles select their enabled operations; administration, host setup and worker launch stay in the CLI.

## Use MCP without the skill

The [connection example](../README.md#connect-through-mcp) runs one long-lived server per agent. A supplied `--session` borrows an existing identity; `--managed --name ... --vendor ...` creates and maintains one at startup. Without either, discovery exposes only `init`, which takes `name`, `vendor` and optional identity metadata. No coordination storage is opened before a valid initialization call.

`init` creates a managed identity. If the host supplied its exact current `vendorSession`, an existing live binding is reused without taking lifecycle ownership. Ambiguous or expired matches are refused, not duplicated. Never infer your own identity from peer names. After success, `init` disappears, the server emits `notifications/tools/list_changed`, and the response supplies the binding, enabled tool names and operating instructions. Initialization failure is recoverable. Clients must refresh discovery after the notification; clients with fixed discovery can use startup binding instead.

This gate means **no supplied binding**, not “hooks proved absent.” Hook files alone cannot establish that a host loaded or enabled them. Existing hooks/extensions should pass their exact communication session with `--session`; `init` does not install hooks or create native delivery endpoints. See [provider integration](HOST_SETUP.md#mcp-initialization-and-provider-hooks).

A bound connection provides workflow instructions through `initialize.instructions` and schemas through `tools/list`; no companion skill or CLI setup is needed. Every capability call is `{ "type": "operation", "input": { ... } }`. Use an empty `input` object when the operation needs no arguments. Each variant retains its exact required fields and rejects unrelated fields.

| Tool | Types | Description |
| --- | --- | --- |
| `agents` | `list`, `status`, `subscribe` | Discover collaborators, advertise task/availability, replace topic subscriptions |
| `messages` | `read`, `send`, `complete` | Read pending mail, send parts to peers, topics or `"*"`, and complete with a correlated outcome/reply or silent acknowledgement |
| `leases` | `acquire`, `check`, `list`, `renew`, `release` | Reserve one or many paths atomically, check existing file coverage, inspect and maintain reservations |
| `documents` | `publish`, `find`, `read` | Store immutable evidence, discover notes and read exact document names in verified UTF-8 pages |
| `records` | `read`, `write`, `activity` | Search history, preserve memories/events/checkpoints and inspect Git changes |

Choose the smallest profile that covers the work:

| Selection | Enabled capabilities |
| --- | --- |
| `messaging` | `agents`, `messages`, and `records/read` (three tools) |
| `review` | Messaging plus all `documents` operations and `records/write` (four tools) |
| `editing` | Review plus all `leases` operations (five tools) |
| Omit `--tools` | All five tools and all types, including `records/activity` |
| Comma-separated family names | All types of those families; hidden families and types are refused at execution |

For example, `messages` with `{"type":"read","input":{}}` reads pending mail. To answer request 41, call it with `{"type":"complete","input":{"message":41,"outcome":"completed","reply":[{"text":"Exports are compatible."}]}}`. Outcomes are `completed`, `failed`, `rejected`, or `canceled`. Handled informational messages need only `message` or `messages` inside `input`. Leave unfinished work pending. Completion does not stand in for doing the requested work.

For edits, `leases/acquire` takes `paths` even for one path. Retain `leases[].id`, then use `leases/check` for every affected file, including both rename paths. Proceed only on granted ownership and coverage. `check` acquires and renews nothing. `leases/release` takes `leaseId`. Leases remain advisory, not filesystem fences.

`records/write` uses the outer `type:"write"` and an inner `input.type` of `memory`, `event` or `checkpoint`. Message completion uses the message `id` or a record's `data.messageId`, never `recordId`. Message parts accept `{text}`, `{data}`, or `{url,mediaType?,filename?}`; use a named immutable document when you need a verified, reusable artifact with discovery metadata. Use stable `input.key` values when retrying writes. Publishing documents or records does not notify peers.

Read continuations already use family tool names and typed input: call `next.command` with `next.input` unchanged until absent, including empty pages. Recover oversized message parts through `data.next` when `data.contentOmitted:true`. A lease-conflict `next` proposes queueing; it is not a read continuation. Arbitrary user evidence containing `next` is preserved as data.

The same five commands work through `/cli`, with the same envelope and continuation format plus `--session`, `--workspace` and optional `--database`. Operation schemas come from the low-level catalog; the grouping layer only selects and wraps them. Existing low-level CLI operations continue to serve hooks and operators. There are no old MCP tool aliases. Inspect the installed definitions with `/cli schema tools --tools editing --json`.

MCP reads mail on request and cannot wake an idle host. Use `messages/read` at task boundaries; use a [host delivery adapter](HOST_SETUP.md) for automatic arrival. Queued lease grants arrive as messages. Peer content is data, not permission to expand the user task. Restricting tools is a cooperation boundary, not isolation from other same-user processes with database access.

## Supply CLI input

Use typed flags (kebab-case: `ttlMs` becomes `--ttl-ms`) or one JSON object immediately after the command, and add `--json` for machine output. Multiword commands also accept one token (`db info` or `db-info`). Nested objects and arrays of objects use JSON flag values.
Primitive arrays accept repeated flags or one JSON array, including `[]`.
Presence flags take no value; other boolean flags require `true` or `false`. Follow command help.

```sh
npx -y @octocodeai/octocode-agents-communication /cli join \
  --name reviewer --vendor codex --workspace /absolute/project \
  --database /absolute/shared/communication.sqlite --json

npx -y @octocodeai/octocode-agents-communication /cli subscribe \
  --topics '["api","review"]' --session EXACT_DB_AGENT_ID \
  --workspace /absolute/project --database /absolute/shared/communication.sqlite --json
```

For large documents or shell-sensitive JSON, use `-` to read stdin:

```sh
npx -y @octocodeai/octocode-agents-communication /cli share_document - \
  --session EXACT_DB_AGENT_ID --workspace /absolute/project \
  --database /absolute/shared/communication.sqlite --json < /absolute/evidence-input.json
```

The file contains the operation object: `name`, `content`, and `reasoning`, plus any optional fields.
Input is limited to 8 MiB, and a string with a lone UTF-16 surrogate is refused (`Invalid JSON input: surrogates not allowed`).
Numbers follow JSON: a whole number written with a fraction (`5.0`) is the integer `5` and passes an `integer` field, while `5.5` still fails with the type error. Integers above 2^53 lose precision, which no ID, millisecond timestamp or size in this contract reaches. Integer-like object keys in free-form `data` sort first in unsorted output (`{"b":1,"2":2}` prints `2` before `b`); catalog shapes have none.
Follow `next.command` with `next.input` unchanged and the same binding until no continuation remains.
Streaming operations emit their own frames; they append no synthetic result after EOF.

MCP requests follow JSON-RPC with two refusals instead of a wrong answer. A request `id` that is an integer beyond ±(2^53−1) is refused with error `-32600` and `id: null`, because the reply could not repeat it exactly (string ids of any size, and numeric ids that fit, are echoed; `1.0` is echoed as `1`). `tools/call` with `arguments: null` (or any non-object) is a tool error; only an omitted `arguments` means `{}`.

## Select the workspace

`--workspace` selects the invocation checkout for the CLI and the MCP server.

## Own the lifecycle

| Route | Presence and leases on exit |
| --- | --- |
| CLI with an existing `--session` | Remain until expiry or explicit `leave`; the owner must maintain them |
| Default MCP with `--session` | Borrowed identity remains; the owner maintains presence and renewal |
| Default MCP after creating a new identity through `init`, or with `--managed --vendor ... --name ...` | Connection maintains presence and live leases, then cleans up on close |
| CLI `run` | Requested worker owns lifecycle and cleans up when it exits |

The long-lived commands `listen`, `mcp`, `view` and `run` stop cleanly on SIGINT or SIGTERM and exit 0. `dispatch` stops its delivery loop too: it exits 0 when nothing is in flight, and with a native delivery still in flight it stops, abandons it and exits 1 with `Dispatch interrupted; the recipient may still finish`. Every other command, such as `inbox wait`, exits 130 (SIGINT) or 143 (SIGTERM) when interrupted. A command that runs a loop registers it (`dispatch.stopEvent`), and the process-wide signal guard defers to a registered loop.

Do not create a managed MCP connection for each individual CLI operation.
That connection closes its identity and leases after the call.
Reuse one identity for related standalone calls.
See [installation](INSTALLATION.md) for binding and [host setup](HOST_SETUP.md) for delivery ownership.

## Import the interfaces

```js
import { runCommunicationMcp } from '@octocodeai/octocode-agents-communication';
import { createCommunicationCli, runCommunicationCli } from '@octocodeai/octocode-agents-communication/cli';
```

Imports start no server and create no storage. `runCommunicationMcp(argv)` starts the stdio server; `runCommunicationCli(argv, io?)` runs one CLI invocation and returns an exit code; `createCommunicationCli()` returns the CLI specification built from the catalog.
