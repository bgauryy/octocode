# Octocode agents communication

Use this MCP server and `/cli` interface to coordinate agents across linked Git worktrees.
Codex, Claude Code, Grok Build, Pi, OpenCode, Cursor, and other local agents share the same messaging contract.
One Python runtime and SQLite database provide discovery, requests/replies, advisory path leases, documents, memories, and events.

```mermaid
flowchart LR
  I[Run the CLI] --> B[Bind each agent]
  B --> P[Discover peers]
  P --> S[Send a request]
  S --> H[Recipient handles it]
  H --> C[Complete with reply]
  C --> A[Sender reads and acknowledges]
```

Each agent keeps its own identity. All participants use the same database file. Each identity retains its actual worktree path.
Linked worktrees share a repository scope derived from Git’s canonical common directory.
Independent clones and unrelated non-Git workspaces remain separate.

## What you can do

| Need | Commands and behavior |
| --- | --- |
| Identify collaborators | `join`, `binding`, `peers`, `set_status`, `heartbeat`, `resume`, `leave`: keep a separate live identity for each agent. |
| Request work and finish it | `send_message`, `inbox`, `inbox wait`, `complete`: correlate the final answer and acknowledge handled work. |
| Notify a group | `subscribe` and topic messages, or `notify_all`: reach current subscribers or live peers; later joiners receive no old fanout. |
| Coordinate edits | `lock`, `lock_many`, `renew`, `unlock`, `locks`, `check_paths`, `check_write`: reserve files or trees, queue conflicts, and check ownership. |
| Share larger evidence | `share_document`, `read_document`, `context`: publish immutable documents and discover relevant notes. |
| Query history and findings | `record`, `fetch`, `activity`, `record_usage`: retain memories, events, Git evidence, and measured usage. |
| Connect an agent host | `mcp`, `attach`, `listen`, `dispatch`: expose bound tools or deliver to an existing native session. `run` starts a new managed worker only when requested. |
| Integrate host events | `host-config`, `host-hook`, `hook`, `confirm_delivery`, `completion-check`: preview setup, deliver context, and optionally check unfinished handling. |
| Inspect and recover | `health`, `view`, `retry_delivery`: inspect delivery and explicitly recover uncertain attempts. |
| Maintain storage | `db info`, `db protocol`, `db export`, `db retention`, `db compact`, `db migrate`, `prune`: inspect, back up, migrate, and maintain storage without dropping message history. |
| Discover inputs | `skill`, `schema`, `schema tools`, `schema types`, `schema type`: read operating guidance and the installed release's contracts. |

Use [the command reference](scripts/docs/COMMANDS.md) for examples and `<command> --help` for exact inputs. Choose the relevant feature; setup does not require running every command.

## Choose an entry point

| Entry | Behavior | Identity lifecycle |
| --- | --- | --- |
| Default npm executable | MCP server over stdio; 18 tools before profile selection | Borrows `--session`, or owns a `--managed` identity |
| npm executable with `/cli` | 50 operations and two record-type discovery commands | Separate calls reuse the supplied identity |
| Package import | Exports `runCommunicationMcp` | Starts only when called |
| Package `/cli` import | Exports `createCommunicationCli` and `runCommunicationCli` | Runs operations when called |

## Install

Run the standalone package with npm:

```sh
npx -y @octocodeai/octocode-agents-communication /cli --help
npx -y @octocodeai/octocode-agents-communication /cli skill --json
```

The bare executable starts MCP over stdio and requires an identity; a `--session required` error means none was supplied. Use `/cli` for operations or [configure a managed MCP connection](#connect-through-mcp).

CLI inputs support typed flags or the existing positional JSON object. Add `--json` for machine output.
For `/cli`, use `--workspace-root` for the invocation checkout (`--workspace` remains an alias). The default MCP entry uses `--workspace`.
The CLI bundles `octocode-mcp-cli`; no separate unpublished runtime dependency is needed.
`skill --json` returns `instructions`, `packageRoot`, and `referenceRoot`; read the operating guide and resolve its documentation filenames under `referenceRoot`.

The companion skill is a separate lean guide in `skills/octocode-agents-communication`.
Install it with `npx -y octocode skill install octocode-agents-communication --platform claude,codex --global`, then reload skill discovery.

The npm launcher requires Node.js 24.15+ (24.x) and Python 3.9+ with SQLite 3.42+ and FTS5.
No pip packages or API key are required for the communication service. Set `OCTOCODE_PYTHON` to an interpreter path in the launching environment when needed; the launcher does not read this override from `<HOME>/.octocode/.env`.
The direct Python CLI/MCP also works without Node.js.
See [installation](scripts/docs/INSTALLATION.md) for bindings and portable runtime archives.

## Start with the CLI

Use two participating sessions: `coordinator` sends the request and `api-reviewer` answers it. Repeat the setup below in each session, using its own name and actual host vendor. These POSIX shell helpers are session-local conveniences; on Windows, use the [CLI entry-point examples](scripts/docs/INTERFACES.md).

Replace the workspace and database paths with actual absolute paths. Linked worktrees use their own checkout path and the same database:

```sh
COMMUNICATION_WORKSPACE=/absolute/project
COMMUNICATION_DB=/absolute/shared/communication.sqlite
COMMUNICATION_NAME=coordinator
COMMUNICATION_VENDOR=codex
comm() {
  npx -y @octocodeai/octocode-agents-communication /cli "$@" --json --workspace-root "$COMMUNICATION_WORKSPACE" \
    --database "$COMMUNICATION_DB"
}
comm db info
comm join --name "$COMMUNICATION_NAME" --vendor "$COMMUNICATION_VENDOR"
```

In the recipient session, set `COMMUNICATION_NAME=api-reviewer` before joining. `db info` inspects storage without creating it. Omitting `--database` uses `<HOME>/.octocode/agents-communication/communication.sqlite`, or `<OCTOCODE_HOME>/agents-communication/communication.sqlite` when that override is set. `join` returns this agent's exact database `id` and its final unique name.
Use the actual host as `vendor`; a model name does not identify the host.
If the host already supplies a binding, reuse it and skip `join`.

Set `COMMUNICATION_SESSION` to your returned ID or supplied binding:

```sh
COMMUNICATION_SESSION=EXACT_DB_AGENT_ID
agent() { comm "$@" --session "$COMMUNICATION_SESSION"; }
agent heartbeat '{"ttlMs":600000,"task":"Review API compatibility","status":"busy"}'
comm peers
```

Confirm both sessions appear in `peers`. If a name was already taken, `join` returns a suffixed name; copy that exact name or ID. In `coordinator`, send the request to the recipient:

```sh
agent send_message '{"to":"api-reviewer","body":"Review src/api.ts for compatibility.","reasoning":"Resolve API compatibility before handoff","key":"api-review-1","conversationId":"api-review"}'
```

In the recipient, read pending mail using its own binding:

```sh
agent fetch '{"incoming":true,"type":"message","limit":20}'
```

After doing the requested work, use the request's `items[].data.messageId` as the number passed to `--message` and provide the final answer:

```sh
agent complete --message RECEIVED_MESSAGE_ID --reply 'Reviewed src/api.ts; the response shape is compatible.'
```

`complete` stores the reply and acknowledges the request in one transaction.
In `coordinator`, read and acknowledge the answer without another reply:

```sh
agent fetch '{"incoming":true,"type":"message","limit":20}'
agent complete --message ANSWER_MESSAGE_ID
```

Replace `ANSWER_MESSAGE_ID` with that answer's `data.messageId`.
Use `data.messageId` for fetched records; `recordId` is only a history cursor.
For informational mail (`replyRequired:false`), complete the handled ID without `reply`.
Leave unfinished work pending.

Raw identities expire after 60 seconds by default; the setup heartbeat allows ten minutes for this walkthrough. Renew before expiry, choosing `ttlMs` to cover the next work interval (up to ten minutes). Use `heartbeat {"ttlMs":600000,"renewLeases":true}` to extend presence and live owned leases together.
`inbox wait` is read-only; it never renews presence. Keep a raw wait shorter than remaining presence.
For a maintained tool connection, use [managed MCP](#connect-through-mcp).
When your process owns the lifecycle, run `agent leave` after finishing.

## Coordinate edits and share evidence

Before changing a shared path, acquire a reservation and check `ok:true`:

```sh
agent lock '{"path":"src/api.ts","reasoning":"Implement the reviewed API change"}'
agent check_write '{"paths":[{"path":"src/api.ts"}]}'
```

Retain the returned `lease.id`. Use `lock_many` when several paths must be reserved together. A conflict returns a suggested `next` with `wait:true`; running it queues the request. Continue only after ownership is granted, and keep unfinished messages pending. Release completed work with `agent unlock --lease-id LEASE_ID`. An expired identity must resume and acquire fresh leases. Read [lease and lifecycle details](scripts/docs/WORKFLOW.md) for renewal and queue behavior.

Publish larger evidence once, then notify its reader:

```sh
agent share_document '{"name":"api-review.md","content":"Verified API findings...","reasoning":"Preserve the review evidence","context":{"summary":"API compatibility review","path":"src/api.ts"}}'
agent read_document '{"name":"api-review.md","limit":4096}'
```

Document names are immutable; use a new name for a revision. Follow every read continuation, including empty pages, and verify the retrieved content before relying on it. `context` discovers relevant notes. Saving a document or memory does not notify peers; use `send_message` for a handoff.

## Connect through MCP

Configure the host to launch a separate bound process for each agent:

```sh
npx -y @octocodeai/octocode-agents-communication --managed \
  --name api-reviewer --vendor codex --tools review \
  --workspace /absolute/project --database /absolute/shared/communication.sqlite
```

Managed mode owns identity, presence, live lease renewal, and cleanup on EOF or termination.
It exposes tools for manual inbox reads; it does not push context or wake an idle host.
For a host-owned identity, use the default npm executable with `--session EXACT_DB_AGENT_ID`.
Its owner maintains presence and lease renewal. Closing this borrowed MCP connection does not end the identity.
Closing managed MCP expires its identity and leases.

For a host that accepts an `mcpServers` configuration, merge this entry into its existing settings:

```json
{
  "mcpServers": {
    "communication": {
      "command": "npx",
      "args": [
        "-y", "@octocodeai/octocode-agents-communication",
        "--managed", "--vendor", "codex", "--name", "api-reviewer",
        "--workspace", "/absolute/project",
        "--database", "/absolute/shared/communication.sqlite",
        "--tools", "review"
      ]
    }
  }
}
```

Use the actual vendor and a unique participant name. Each independent agent needs its own identity.
A host with a different configuration format needs the same command and arguments.
MCP writes JSON-RPC to stdout and diagnostics to stderr.

Choose `messaging` for coordination, `review` for documents, or `editing` for leases too.
An explicit comma-separated tool list also works. Bound tools take JSON input without workspace or identity arguments.
Inspect `schema tools --tools review` or one `schema send_message` when you need details.

## Choose delivery for your host

Native attachment targets an existing host session. It creates no worker or additional permissions.
Use one delivery owner for each identity. Skill installation and hook installation are separate steps.

| Host | Automatic delivery | Idle action wake | Edit admission |
| --- | --- | --- | --- |
| Claude Code | Native inbox socket or post-tool hooks | Native policy; hooks do not wake | Optional Write/Edit/MultiEdit/NotebookEdit guard |
| Codex | App-server tool-output delivery or post-tool hooks | Native active/idle turn; hooks do not wake | No bundled guard |
| Grok Build | ACP leader socket or post-tool hooks | Native socket; hooks do not wake | No bundled guard |
| Pi | Inbox extension and durable host receipts | At idle for action mail | `editing` profile |
| OpenCode | Loopback server API | Action uses `prompt_async` | Optional write/edit plugin |
| Cursor | Post-tool hooks | No | No bundled guard |
| Other local agents | CLI/MCP inbox or a host-wired context hook | Host-dependent | Host-dependent |

For hook setup, follow [preview generation, merge locations, trust, and a two-agent delivery check](scripts/docs/HOST_SETUP.md#install-messaging-hooks).
For native endpoints and host-specific receipts, use [the service protocol](scripts/docs/SERVICE_PROTOCOL.md#identity-and-capabilities).
For Pi or structured edit guards, use [lease admission setup](scripts/docs/HOST_LEASE_GUARDS.md).
Without an adapter, read the inbox through CLI/MCP. A database write alone does not wake a process.

`run` can start a new managed Codex, Claude, or Pi worker when requested. Existing sessions use `attach`; use [host setup](scripts/docs/HOST_SETUP.md) for actual endpoints, host credentials, and lifecycle ownership. Optional Claude/Pi completion checks inspect unfinished handling; they do not acknowledge work automatically.

## Query unified records

Messages, coordination, leases, documents, usage, memories, and events share this envelope:
`{recordId,path,from,to,type,timestamp,branch?,data}`.
`path` is the workspace, `from` is the database agent ID, and `timestamp` is Unix milliseconds.
`to` is present and nullable for shared records; `branch` is optional.
Generic memory/event data preserves nested JSON, nulls, and booleans.

Discover every type/field compactly or inspect one full type schema:

```sh
npx -y @octocodeai/octocode-agents-communication /cli schema types --compact --json
npx -y @octocodeai/octocode-agents-communication /cli schema type message --json
agent record '{"type":"memory","branch":"feature/api","data":{"content":"API preserves the response shape","tags":["api"],"source":"src/api.ts"}}'
agent fetch '{"type":"memory","search":"response shape","branch":"feature/api","limit":20}'
```

The explicit branch labels the record; it does not switch the Git checkout.
Record-type discovery needs no binding; version 0.1.0 rejects workspace, database, or session flags on `schema types` and `schema type`, so call them directly as shown.
Record `path` retains the originating worktree, even when another worktree reads it.
`record` saves repository-shared memories/events without notifying peers. Use `send_message` when another agent must act.
Narrow fetches by type, sender/recipient, branch, time, text, or scalar payload fields.
Follow each `next.command` with `next.input` unchanged and the same binding until no continuation remains.
Oversized hook references carry `data.bodyOmitted:true` and `data.next` for the intact record; fetch it before handling.

Operational tables enforce current state and transactions alongside the append-only history.
Existing v1/v2/v3/v4 stores need an explicit, backed-up migration with old clients stopped.
The [database protocol](scripts/docs/DB.md) explains every type, query, transaction, and migration rule.

## Inspect and recover

`comm health` reads delivery diagnostics without joining or changing storage.
Inspect its status and follow issue continuations; process exit success alone does not mean healthy delivery.
For an authorized workspace-wide audit, use the local dashboard:

```sh
comm view '{"open":true}'
```

The dashboard reads an existing database, uses a loopback URL token, and stops with Ctrl+C.
It shows full paginated history and operational views. Participant-scoped `fetch` exposes only permitted records.
[Operations and recovery](scripts/docs/OPERATIONS.md) explains uncertain offers, expiry, snapshots, and retention.

`db export` writes a verified snapshot to a new absolute path. Document files need a separate backup. `db retention` reports aged records and `db compact` reclaims unused space while retaining history; `prune` removes expired leases. Stop old clients before an explicit `db migrate`. If an existing WAL database rejects your Python runtime, follow the reported SQLite upgrade guidance and use the same compatible interpreter for every writer and hook.

Only a successful `lock`/`lock_many` grants an advisory reservation.
Leases, Git activity, and native host validation remain worktree-local.
Optional edit guards cover their documented structured operations, not arbitrary shell or OS writes.
Submission is not handling; acknowledgement is not proof of correctness or user approval.

## Documentation map

| Document | Reader and purpose |
| --- | --- |
| [OPERATING.md](OPERATING.md) | Compact operating workflow for agents; managed workers use this same source |
| [Installation](scripts/docs/INSTALLATION.md) | Install the standalone bundle and create a raw binding |
| [CLI and MCP entry points](scripts/docs/INTERFACES.md) | Choose a route, use typed flags or stdin, and understand lifecycle |
| [Command reference](scripts/docs/COMMANDS.md) | Find every command and validated input example |
| [Workflow details](scripts/docs/WORKFLOW.md) | Check message examples, retry keys, TTLs, and topics |
| [Record queries](scripts/docs/RECORDS.md) | Distinguish IDs and query typed records |
| [Host setup](scripts/docs/HOST_SETUP.md) | Configure lifecycle owners, tool profiles, and messaging hooks |
| [Host hooks](scripts/docs/HOST_HOOKS.md) | Integrate event payloads, context budgets, and generations |
| [Lease guards](scripts/docs/HOST_LEASE_GUARDS.md) | Configure structured-edit admission and understand its limits |
| [Database protocol](scripts/docs/DB.md) | Implement SQL clients and inspect schema, visibility, paging, and migrations |
| [Service protocol](scripts/docs/SERVICE_PROTOCOL.md) | Implement native delivery and interpret transport receipts |
| [Operations](scripts/docs/OPERATIONS.md) | Diagnose delivery and preserve or recover storage |
| [Architecture](ARCHITECTURE.md) | Maintain module boundaries, contracts, and packaging |

## Build and validate

From the monorepo root, use the package's maintainer scripts:

```sh
yarn workspace @octocodeai/octocode-agents-communication build
node packages/octocode-agents-communication/bin/octocode-agents-communication.mjs /cli --help
yarn workspace @octocodeai/octocode-agents-communication verify
yarn workspace @octocodeai/octocode-agents-communication pack:runtime
```

The build refreshes the shared config runtime and validates startup.
Verification runs syntax, links, Python/Node regressions, and extracted CLI/MCP/reply/lease/export/restore checks.
Packing writes a portable archive under the package's `out/` directory.
The npm archive includes `dist/`, `bin/`, `scripts/`, the README, architecture guide, license, and `OPERATING.md`. Portable runtime archives contain `scripts/` and `OPERATING.md`.
The installable skill contains `SKILL.md`, `README.md`, and `output.md`; `src/` and `tests/` are maintainer-only.
Local verification covers its own platform, not every host or operating system.

To test an unpublished npm archive outside the repository:

```sh
npx -y --package /absolute/package.tgz octocode-agents-communication /cli --help
```

For an optional live model check, run from the monorepo root:

```sh
node packages/octocode-agents-communication/src/live-host-smoke.mjs --vendor codex --output /absolute/new-results
```

The runner creates isolated identities and storage, uses the configured Codex model, and writes delivery, reply, acknowledgement, and token receipts.
For Pi or Claude, supply `--model` with an available configured model ID. It changes no host settings.
Protocol fixtures prove contract behavior; this live smoke checks the installed host and model.
A single passing run does not establish reliability across models or hosts.

For a reliability measurement, run N independent trials of three flows (question and answer, lock conflict and handoff, paged document read), each with a fresh workspace, database and managed worker:

```sh
node packages/octocode-agents-communication/src/live-reliability.mjs --vendor claude --model MODEL_ID --runs 10 --output /absolute/new-results
```

It writes one JSON file per trial (checks, tool-error counts, trace) and `summary.json`. The summary passes when every flow passes at least 90% of trials with zero rejected `complete` calls.
