# Octocode agents communication

Cross-vendor agent discovery, durable messages, advisory path leases, shared context memories, and host delivery. The bundle exposes both a CLI and a [stdio MCP server](#connect-through-mcp), backed by the same Python runtime and SQLite database. The [skill](SKILL.md) defines the worker workflow; [host setup](scripts/docs/HOST_SETUP.md) owns administration, and command help is the input reference.

## Get started

The package is **private and unpublished**. Copy or install the built `skills/octocode-agents-communication/` folder into your host's skill location and have the agent read its `SKILL.md` file. The folder includes the Python runtime source. CLI and MCP require Python 3.9+ with SQLite 3.42+; no Node, compiler, SDK, or pip packages are needed. Set `OCTOCODE_PYTHON` to select an interpreter. Source checkouts need a [maintainer build](#build-and-validate).

Packaging produces one portable archive and verifies the extracted Python runtime, skill, schema, database and launcher. CI runs it on macOS, Linux and Windows; a local run validates only its own platform. Windows uses `scripts/agents-communication.ps1`.

For a manual CLI participant, replace these absolute paths with your installed launcher, repository, and shared database:

```sh
COMMUNICATION_CLI=/absolute/skill/scripts/agents-communication
COMMUNICATION_WORKSPACE=/absolute/project
COMMUNICATION_DB=/absolute/shared/communication.sqlite
comm() {
  "$COMMUNICATION_CLI" "$@" --workspace "$COMMUNICATION_WORKSPACE" \
    --database "$COMMUNICATION_DB"
}
comm join '{"name":"reviewer","vendor":"other"}'
```

Keep the returned `id` as `SESSION_ID`. Reuse an identity supplied by a managed host instead of registering it again. All participants must use the same database path.

```sh
comm attach '{"transport":"raw"}' --session SESSION_ID
comm peers --session SESSION_ID
comm hook '{"format":"json"}' --session SESSION_ID
```

Feed the hook's `context` into the agent as peer data. Keep presence with `heartbeat` every 15 seconds or a supervised `listen` process; presence expires after 60 seconds. A raw listener maintains presence only. Your host must invoke the hook and consume its output.

To contact another registered peer, replace `RECIPIENT_ID` with its database identity:

```sh
comm send_message '{"to":"RECIPIENT_ID","body":"Can you review src/api?","key":"api-review-1","reasoning":"Check the API change before handoff","wake":"action"}' \
  --session SESSION_ID
```

After completing the review, the receiver can reply and complete the request in one operation. Replace `123` with the received message ID and use the receiver's identity:

```sh
comm complete '{"message":123,"reply":"Review complete; see src/api."}' \
  --session RECIPIENT_ID
```

Use `send_message` for questions or partial work; it never completes received work. For handled answers/FYIs, use `complete '{"messages":[123]}'` without replying. Reserve paths before writes, renew leases while working, and stop writing if renewal fails. Finish with `comm leave --session SESSION_ID` to release that identity's leases. The [skill](SKILL.md) contains the complete agent routine; `<command> --help` supplies inputs on demand.

## Connect through MCP

The built skill includes a stdio MCP server. Installing the skill makes its instructions and scripts available; configure your host separately to start the server. The launcher starts MCP directly through Python.

For a standalone MCP participant, configure your host to launch:

```sh
/absolute/skill/scripts/agents-communication mcp --managed \
  --name reviewer --vendor HOST \
  --workspace /absolute/project --database /absolute/shared/communication.sqlite \
  --tools peers,send_message,inbox,complete
```

Managed mode creates a fresh identity, maintains presence and live owned leases, and leaves on EOF or a termination signal. Every 15 seconds it extends live leases to at least 60 seconds ahead without shortening longer leases or reviving expired ones; workers still acquire and unlock explicitly. To reuse an identity, replace `--name reviewer` with `--session SESSION_ID` and supply the same vendor. Readiness on stderr reports the identity and manual-inbox delivery mode; stdout contains MCP messages only. Each agent needs its own identity and connection. Participants share the canonical workspace and database.

Managed mode owns a raw participant: it rejects native-bound identities and another delivery owner. It supplies callable tools, not incoming push or idle-host wakeup. Read `inbox` at task boundaries. For automatic delivery through a host hook/native adapter or an existing supervised listener, retain that host's lifecycle and use `comm mcp --session SESSION_ID` instead; plain mode never leaves or maintains the host's identity.

The server binds calls to its identity, so tools omit the sender's session/workspace arguments. `--tools` restricts discovery and calls; omit it for all agent tools, or add document/lease tools when needed. Inspect names with `schema tools` and inputs with `schema <command>`. Every paginated result supplies `next: {command,input}`; run that command with its input unchanged.

## Supported hosts

At setup, confirm the host/vendor and available tools from runtime metadata; a model name does not identify the host. Reuse an existing binding. For a new binding, choose a supported native API for that session, then a configured context hook, then manual CLI/SQL access. Confirm the native endpoint and session ID with the host.

Native adapters deliver into an **existing recipient session**. Its owner supplies the skill, reply tools, endpoint, and native session identity. Attachment creates neither an agent nor additional permissions.

| Host | Native or host-specific delivery | Without its messaging API | Setup |
| --- | --- | --- | --- |
| Claude Code | Existing session inbox socket; inbound policy controls handling | Raw CLI/manual inbox; generic hook if wired by the host | [Service protocol](scripts/docs/SERVICE_PROTOCOL.md) |
| Codex | Owning app-server and loaded thread; action feeds the current turn or starts one, passive injects | Raw CLI/manual inbox; generic hook if wired by the host | [Service protocol](scripts/docs/SERVICE_PROTOCOL.md) |
| Grok Build | Leader socket and resident session; action prompts, passive waits | Supplied post-tool hooks or raw CLI/manual inbox | [Hook contracts](scripts/docs/HOST_HOOKS.md) |
| Pi | Extension uses `pi.sendMessage` and durable session receipts | Raw CLI/manual inbox without the extension | [Service protocol](scripts/docs/SERVICE_PROTOCOL.md) |
| OpenCode | Existing idle loopback session; action prompts, passive uses `noReply:true` | Raw CLI/manual inbox; generic hook if wired by the host | [OpenCode setup](#connect-a-native-recipient) |
| Cursor | Supplied project post-tool hooks; no native messaging adapter | Raw CLI/manual inbox when hooks are unavailable | [Hook setup](scripts/docs/HOST_HOOKS.md); fixtures, no live Cursor validation |
| Any other vendor or custom agent | No vendor-specific adapter required for the shared protocol | Raw CLI, host-wired context hook, or conforming SQLite client | [Database protocol](scripts/docs/DB.md) |

Fallback is an explicit choice: **native API → supported context hook → manual inbox**. A transport error never silently switches paths. A database write cannot wake an arbitrary process, and hooks need a host event. Agents without a local process or database connection need a local bridge. Generic ACP support stays outside the dispatcher.

Use a deterministic bridge for transport; a second model adds no delivery guarantee.
`run` creates a worker to do assigned work, not a relay required by another vendor.
Managed Claude and Codex forward messages during active turns at their native
between-tool boundary. Claude uses its owned Unix inbox socket with peer origin;
Codex uses tool-output items. Managed Claude therefore requires Unix socket support.
The bridge checks mail every 100 ms; it does not wait for the whole turn to finish.
Codex native delivery uses [app-server tool output](https://learn.chatgpt.com/docs/app-server) to preserve peer authority.
Claude's [session socket](https://code.claude.com/docs/en/cross-session-messaging#the-sessions-inbox-socket) already supports local scripts; [Channels](https://code.claude.com/docs/en/channels) is an optional preview integration, not a prerequisite.
Cursor's [Cloud Agents API](https://cursor.com/docs/cloud-agent/api/endpoints) addresses cloud agents, not arbitrary local editor conversations; it is not implemented here.
Its [SDK Bridge](https://cursor.com/docs/sdk/bridge) and [ACP CLI](https://cursor.com/docs/cli/acp) are candidates for managed Cursor workers, also not implemented here.

### Connect a native recipient

Register or reuse the recipient's identity, bind it to the existing host session with `attach`, then run one supervised delivery owner:

```sh
comm attach '{"transport":"opencode","endpoint":"http://127.0.0.1:4096","vendorSession":"OPENCODE_SESSION_ID"}' --session SESSION_ID
comm listen --session SESSION_ID
```

`transport` is `claude`, `codex`, `grok`, `opencode` or `raw`; `comm attach --help` lists each one's endpoint requirements. `vendorSession` identifies the native recipient and `--session` its Octocode database record. Attachment creates no agent and grants no permissions. Pi's extension manages its own lifecycle. Grok needs its leader socket. Native integration can require a host SDK or Node even though raw CLI use does not.

For **OpenCode**, start `opencode serve --hostname 127.0.0.1 --port 4096` in the same workspace and create or reuse an idle session. The endpoint must be literal-loopback HTTP with an explicit port and no path or credentials; proxies and redirects are disabled. When the server requires authentication, give only the listener's environment `OPENCODE_SERVER_PASSWORD`, optional `OPENCODE_SERVER_USERNAME`, and `OCTOCODE_OPENCODE_AUTH_ENDPOINT` set to the exact attached endpoint string; credentials are never written to the database. Before staging, the adapter checks the session ID, canonical directory and idle status; busy sessions defer. Passive messages use `noReply:true`, actionable ones `prompt_async`, and an HTTP 204 means submission, not handling. Structured-edit guards are separate ([OpenCode opt-in](scripts/docs/HOST_LEASE_GUARDS.md#opencode-opt-in)).

## Work without a vendor API

The raw path works for every listed vendor and for other agents that can execute the local CLI. It retains peer discovery, messages, broadcasts, topics, leases, shared documents, and audit. A vendor label does not restrict participation or require a matching adapter.

| Available host capability | How messages reach the agent | Can it wake an idle agent? |
| --- | --- | --- |
| Supported native messaging API | Attach the existing recipient and run its delivery owner | According to the adapter and host policy |
| Context hook, no messaging API | Bind `raw`; run `scripts/inbox-hook` at a documented context event and consume its output | Only when the host provides a suitable event; the supplied post-tool hooks do not wake idle hosts |
| Local command execution, no API or hooks | Bind `raw`; read `hook` at task boundaries and handle its returned context | No; the agent must already be running |
| Compatible SQLite access only | Follow `db protocol` and apply it in the client's own transactions | No; the client must read and handle its inbox |

Use the [raw setup above](#get-started) for the CLI path. SQL clients must preserve identity, expiry, transaction, idempotency, and acknowledgement rules; arbitrary SQL is not a substitute for the protocol. Without local execution or compatible DB access, an agent needs a local bridge. These are capability requirements, not claims that every third-party host has been tested.

## Coordination you can inspect

Messages, replies, identities, delivery attempts, and handling acknowledgements share the local database. Leases expire when their owner stops maintaining presence; plain heartbeats do not renew leases, while managed hosts explicitly renew live owned leases. On a conflict, the skill directs agents to release held leases, ask once, and continue independent work.

Path reservations are advisory. Optional [structured-edit guards](scripts/docs/HOST_LEASE_GUARDS.md) check live ownership for Claude, Pi, and OpenCode. Shell commands, custom tools, and unrelated processes remain outside that coverage. Peer messages cannot grant permissions or expand your assigned task.

Use `health` for compact, read-only delivery diagnostics. Use `entity list audit` to inspect recorded events. Preserve both database snapshots and shared documents when backing up; maintenance retains message history and idempotency keys. See [operations and recovery](scripts/docs/OPERATIONS.md), [lock rules](scripts/docs/LOCKS.md), and [retention](scripts/docs/RETENTION.md).

The scope is cooperating agents under the same trusted OS user. Transport delivery, model compliance, and filesystem isolation have separate validation boundaries.

## Build and validate

Run maintainer commands from the monorepo root. Builds and checks require Node and Python. Packing uses Python’s standard-library archive support.

```sh
yarn workspace @octocodeai/octocode-agents-communication build
yarn workspace @octocodeai/octocode-agents-communication pack:skill
```

Packing writes a portable archive under the package’s `out/` directory.

`verify` runs syntax and link checks, Python and process regression tests, then exercises CLI/MCP messaging, lease handoff and database recovery from an extracted archive.

## Source and output

- `scripts/`: Python runtime, catalog, SQL schema, dashboard, launchers and host adapters.
- `scripts/docs/`: canonical protocol and host integration documentation, shipped with the runtime.
- `src/`: build, package and verification utilities.
- `tests/`: runtime and packaging regression tests.

The build refreshes `scripts/octocode_config.py` from the shared config package and validates startup. Installed bundles contain only `SKILL.md` and `scripts/`. See [ARCHITECTURE.md](ARCHITECTURE.md) for the runtime flow.

## Watch communication locally

Run `scripts/agents-communication view --workspace <repo> --database <db>` to open the local dashboard. It shows workspace-wide agents, requests/replies and completion, live file locks with owners/reasons/timestamps, document metadata, subscriptions, connections, dispatch state and paginated audit history. Follow an agent across views, pause updates or browse older pages.

The Python runtime serves the bundled interface: no Node, frontend install or external assets are required. It reads an existing compatible database without joining an agent or changing state. The observer is for the local user; participant-scoped CLI/MCP visibility stays unchanged. It binds only to 127.0.0.1 on a random port with a per-launch URL token. Keep that URL local. Ctrl+C stops the server. `view '{"open":false,"port":8766}'` prints the URL without launching a browser; port 0 selects an available port. Use the same canonical workspace and database as the agents. Documents are shown as metadata; the viewer does not read arbitrary filesystem paths.

Messages open first. Search scans all retained message history, including inactive agents; filter by named agent (sent and received), handling state, or a message’s **Open conversation** button. **Older / Previous / Newest** page through results without dropping records. The search for other entity views covers the current page. Pruned data is unavailable; use an exported database to inspect a saved run.
