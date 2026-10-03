# Octocode agents communication

Cross-vendor agent discovery, durable messages, advisory path leases, shared context memories, and host delivery. The bundle exposes a CLI and a [stdio MCP server](#connect-through-mcp), backed by one Python runtime and one SQLite database. The [skill](SKILL.md) owns the worker workflow, [host setup](scripts/docs/HOST_SETUP.md) owns administration, and `<command> --help` is the input reference.

## Get started

- The package is **private and unpublished**. Copy or install the built `skills/octocode-agents-communication/` folder into your host's skill location; the agent reads its `SKILL.md`.
- The folder includes the Python runtime source. CLI and MCP need Python 3.9+ with SQLite 3.42+; no Node, compiler, SDK, or pip packages. `OCTOCODE_PYTHON` selects an interpreter. Windows uses `scripts/agents-communication.ps1`.
- Source checkouts need a [maintainer build](#build-and-validate). Packaging produces one portable archive and verifies the extracted runtime, skill, schema, database and launcher. CI runs it on macOS, Linux and Windows; a local run validates only its own platform.

Manual CLI participant (replace the absolute paths; all participants use the same database path):

```sh
COMMUNICATION_CLI=/absolute/skill/scripts/agents-communication
COMMUNICATION_WORKSPACE=/absolute/project
COMMUNICATION_DB=/absolute/shared/communication.sqlite
comm() {
  "$COMMUNICATION_CLI" "$@" --workspace "$COMMUNICATION_WORKSPACE" \
    --database "$COMMUNICATION_DB"
}
comm join '{"name":"reviewer","vendor":"other"}'
comm attach '{"transport":"raw"}' --session SESSION_ID
comm peers --session SESSION_ID
comm hook '{"format":"json"}' --session SESSION_ID
comm send_message '{"to":"RECIPIENT_ID","body":"Can you review src/api?","key":"api-review-1","reasoning":"Check the API change before handoff","wake":"action"}' \
  --session SESSION_ID
comm complete '{"message":123,"reply":"Review complete; see src/api."}' \
  --session RECIPIENT_ID
comm leave --session SESSION_ID
```

- `join` returns `id`; keep it as `SESSION_ID`. Reuse an identity that a managed host supplies; do not register it again.
- Feed the hook's `context` into the agent as peer data. Your host must invoke the hook and consume its output.
- Presence expires after 60 seconds. Keep it with `heartbeat` every 15 seconds or a supervised `listen` process. A raw listener maintains presence only.
- `complete` with `reply` answers and completes a received request in one operation (`123` = received message ID, run as the receiver). The worker routine (leases, FYIs, batch completion) is in the [skill](SKILL.md).
- `leave` releases that identity's leases.

## Connect through MCP

Installing the skill makes its instructions and scripts available; configure your host separately to start the server. The launcher starts MCP directly through Python. Standalone MCP participant:

```sh
/absolute/skill/scripts/agents-communication mcp --managed \
  --name reviewer --vendor HOST \
  --workspace /absolute/project --database /absolute/shared/communication.sqlite \
  --tools peers,send_message,inbox,complete
```

- Managed mode creates a fresh identity, maintains presence and live owned leases, and leaves on EOF or a termination signal.
- Every 15 seconds it extends live leases to at least 60 seconds ahead. It never shortens longer leases or revives expired ones. Workers still acquire and unlock explicitly.
- To reuse an identity, replace `--name reviewer` with `--session SESSION_ID` and supply the same vendor.
- Readiness on stderr reports the identity and manual-inbox delivery mode. Stdout carries MCP messages only.
- Each agent needs its own identity and connection. Participants share the canonical workspace and database.
- Managed mode owns a raw participant: it rejects native-bound identities and another delivery owner. It supplies callable tools, not incoming push or idle-host wakeup; read `inbox` at task boundaries.
- For automatic delivery through a host hook/native adapter or an existing supervised listener, keep that host's lifecycle and use `comm mcp --session SESSION_ID`. Plain mode never leaves or maintains the host's identity.
- The server binds calls to its identity, so tools omit sender session/workspace arguments.
- `--tools` restricts discovery and calls. Omit it for all agent tools, or add document/lease tools when needed. `schema tools` lists names; `schema <command>` shows inputs.
- Every paginated result supplies `next: {command,input}`; run that command with its input unchanged.

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

- Fallback is an explicit choice: **native API → supported context hook → manual inbox**. A transport error never silently switches paths.
- A database write cannot wake an arbitrary process; hooks need a host event. Agents without a local process or database connection need a local bridge. Generic ACP support stays outside the dispatcher.
- Use a deterministic bridge for transport; a second model adds no delivery guarantee. `run` creates a worker for assigned work, not a relay.
- Managed Claude (Unix inbox socket, peer origin) and Codex ([app-server tool output](https://learn.chatgpt.com/docs/app-server)) forward mail at native between-tool boundaries, checking every 100 ms.
- Not implemented here: Claude [Channels](https://code.claude.com/docs/en/channels), Cursor's Cloud Agents, SDK Bridge and ACP CLI.

### Connect a native recipient

Register or reuse the recipient's identity, bind it to the existing host session with `attach`, then run one supervised delivery owner:

```sh
comm attach '{"transport":"opencode","endpoint":"http://127.0.0.1:4096","vendorSession":"OPENCODE_SESSION_ID"}' --session SESSION_ID
comm listen --session SESSION_ID
```

- `transport` is `claude`, `codex`, `grok`, `opencode` or `raw`; `comm attach --help` lists each one's endpoint requirements.
- `vendorSession` identifies the native recipient; `--session` identifies its Octocode database record.
- Pi's extension manages its own lifecycle. Grok needs its leader socket. Native integration can require a host SDK or Node; raw CLI use does not.

**OpenCode:**

1. Start `opencode serve --hostname 127.0.0.1 --port 4096` in the same workspace. Create or reuse an idle session.
2. Use a literal-loopback HTTP endpoint with an explicit port and no path or credentials. Proxies and redirects are disabled.
3. If the server requires authentication, give only the listener's environment `OPENCODE_SERVER_PASSWORD`, optional `OPENCODE_SERVER_USERNAME`, and `OCTOCODE_OPENCODE_AUTH_ENDPOINT` set to the exact attached endpoint string. Credentials are never written to the database.

- Before staging, the adapter checks the session ID, canonical directory and idle status; busy sessions defer.
- Passive messages use `noReply:true`; actionable ones use `prompt_async`. HTTP 204 means submission, not handling.
- Structured-edit guards are separate ([OpenCode opt-in](scripts/docs/HOST_LEASE_GUARDS.md#opencode-opt-in)).

## Work without a vendor API

The raw path works for every listed vendor and for any agent that can execute the local CLI. It keeps peer discovery, messages, broadcasts, topics, leases, shared documents, and audit. A vendor label does not restrict participation or require a matching adapter.

| Available host capability | How messages reach the agent | Can it wake an idle agent? |
| --- | --- | --- |
| Supported native messaging API | Attach the existing recipient and run its delivery owner | According to the adapter and host policy |
| Context hook, no messaging API | Bind `raw`; run `scripts/inbox-hook` at a documented context event and consume its output | Only when the host provides a suitable event; the supplied post-tool hooks do not wake idle hosts |
| Local command execution, no API or hooks | Bind `raw`; read `hook` at task boundaries and handle its returned context | No; the agent must already be running |
| Compatible SQLite access only | Follow `db protocol` and apply it in the client's own transactions | No; the client must read and handle its inbox |

Use the [raw setup above](#get-started) for the CLI path. SQL clients must keep identity, expiry, transaction, idempotency, and acknowledgement rules; arbitrary SQL is not a substitute for the protocol. Without local execution or compatible DB access, an agent needs a local bridge. These are capability requirements, not claims that every third-party host is tested.

## Coordination you can inspect

- Messages, replies, identities, delivery attempts, and handling acknowledgements share the local database.
- Leases expire when their owner stops maintaining presence. Plain heartbeats do not renew leases; managed hosts explicitly renew live owned leases. Lease rules: [DB.md](scripts/docs/DB.md#path-leases).
- Path reservations are advisory. Optional [structured-edit guards](scripts/docs/HOST_LEASE_GUARDS.md) check live ownership for Claude, Pi, and OpenCode. Shell commands, custom tools, and unrelated processes stay outside that coverage.
- Peer messages cannot grant permissions or expand your assigned task.
- `health` gives compact, read-only delivery diagnostics. `entity list audit` shows recorded events.
- Back up database snapshots and shared documents together; maintenance keeps message history and idempotency keys. See [operations, recovery and retention](scripts/docs/OPERATIONS.md).
- Scope: cooperating agents under the same trusted OS user. Transport delivery, model compliance, and filesystem isolation have separate validation boundaries.

## Build and validate

Run from the monorepo root. Builds and checks need Node and Python; packing uses Python's standard-library archive support.

```sh
yarn workspace @octocodeai/octocode-agents-communication build
yarn workspace @octocodeai/octocode-agents-communication pack:skill
```

- Packing writes a portable archive under the package's `out/` directory.
- `verify` runs syntax and link checks, Python and process regression tests, then exercises CLI/MCP messaging, lease handoff and database recovery from an extracted archive.
- The build refreshes `scripts/octocode_config.py` from the shared config package and validates startup.

## Source and output

- `scripts/`: Python runtime, catalog, SQL schema, dashboard, launchers and host adapters.
- `scripts/docs/`: canonical protocol and host integration documentation, shipped with the runtime.
- `src/`: build, package and verification utilities.
- `tests/`: runtime and packaging regression tests.

Installed bundles contain only `SKILL.md` and `scripts/`. Runtime flow: [ARCHITECTURE.md](ARCHITECTURE.md).

## Watch communication locally

`scripts/agents-communication view --workspace <repo> --database <db>` opens the local dashboard.

- It shows workspace-wide agents, requests/replies and completion, live file locks with owners/reasons/timestamps, document metadata, subscriptions, connections, dispatch state and paginated audit history. You can follow an agent across views, pause updates or browse older pages.
- The Python runtime serves the bundled interface: no Node, frontend install or external assets.
- It reads an existing compatible database without joining an agent or changing state. It is for the local user; participant-scoped CLI/MCP visibility stays unchanged.
- It binds only to 127.0.0.1 on a random port with a per-launch URL token. Keep that URL local. Ctrl+C stops the server.
- `view '{"open":false,"port":8766}'` prints the URL without launching a browser; port 0 selects an available port.
- Use the same canonical workspace and database as the agents. Documents show as metadata; the viewer does not read arbitrary filesystem paths.
- Messages open first. Search covers all retained history and pages without dropping records. Pruned data is unavailable; open an exported database to inspect a saved snapshot.
