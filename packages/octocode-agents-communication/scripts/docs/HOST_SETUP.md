# Host setup and administration

Load when configuring identity, delivery, tool profiles, edit guards, or storage. Why: host configuration is separate from ordinary agent work.
Use [installation](INSTALLATION.md) for bundle setup and [the operating guide](../../OPERATING.md) for coordination.
Use [commands](COMMANDS.md) or [records](RECORDS.md) for examples and type lookup.

`<command> '<json>' --workspace <repo> --database <db> --session <id>`; `-` reads stdin. Before unfamiliar CLI calls, read `<command> --help` or `schema <command>`; don't guess JSON fields. Bare `schema` includes full protocol/SQL; workers expose a subset.

| Purpose | Commands |
| --- | --- |
| Identity | `join`, `heartbeat`, `resume`, `leave`, `set_status`; `binding` for adapter preflight |
| Discovery | `peers`, `activity`, `context` |
| Messages | `send_message`, `notify_all`, `subscribe`, `inbox`, `inbox wait`, `complete` |
| Documents | `share_document`, `read_document` |
| Leases | `locks`, `lock`, `lock_many`, `renew`, `unlock`, `check_paths`, `check_write` |
| Delivery | `attach`, `listen`, `dispatch`, `hook`, `confirm_delivery`, `retry_delivery` |
| Hosts | `mcp`, `run`, `host-hook`, `host-config`, `completion-check`, `record_usage` |
| Local dashboard | `view` |
| State | `fetch`, `record`, `health` |
| Database | `db info`, `db protocol`, `db migrate`, `db export`, `db retention`, `db compact`, `prune` |
| Help | `skill`, `schema`, `schema tools` |
Records: `schema types --compact` inventories all fields; `schema type <type>` gives schema, event meaning, routing and a validated example. These discovery commands open no database.

## Setup

The default npm command serves MCP. Its `/cli` route exposes standalone operations.
For npm host configuration, use the [README MCP example](../../README.md#connect-through-mcp).
The direct Python launcher still requires its explicit `mcp` command, as shown below.
See [entry points](INTERFACES.md) for the difference between borrowed and managed identity.
MCP uses stdio: the host launches the command below; JSON-RPC on stdout, diagnostics on stderr. One bound process per agent.
For standalone MCP, launch `scripts/agents-communication mcp --managed --tools messaging --name <agent> --vendor <host> --workspace <repo> --database <db>`: it owns identity, presence, live lease renewal and exit cleanup; read `inbox` at task boundaries. It does not wake idle hosts. With a host-owned identity/delivery owner, use plain `mcp --session <id>`.
Reuse bindings; else `join`, `attach` real host endpoints/IDs. Use one delivery owner. `listen` maintains presence and native delivery; otherwise heartbeat every 15s (60s presence). `OCTOCODE_COMMUNICATION_HEARTBEAT_MS` (50–60000) shortens managed loop periods for tests only; presence and lease TTLs never change. Heartbeats require live presence. They renew live owned leases only with `renewLeases:true`; expired identity requires `resume` and fresh leases.

Managed MCP and `run` workers use `heartbeat {"renewLeases":true}` every 15 seconds. The Pi inbox adapter checks periodically (roughly 15–25 seconds with idle backoff, plus CLI latency): live owned leases extend to at least 60 seconds from that tick, longer leases stay unchanged, and expired leases are never revived. They advertise managed renewal to the worker. Use the default 60-second lease TTL; a custom TTL shorter than a host tick can expire before renewal and must be reacquired. A plain listener or externally bound MCP does not promise lease renewal; its owner must arrange it explicitly. `leave` on completion; `run` only when requested.
Choose `--tools messaging` for coordination, `review` for shared evidence, or `editing` for leases too; explicit comma lists also work. Report supported/configured edit guards; enable them for editing. Guards cover only reported operations, never arbitrary shell/OS writes.
`host-config --help` previews Claude/Codex/Grok/Cursor hooks; Pi uses its extension. Context hooks cannot wake idle hosts. `completion-check` allows one recovery turn, never automatically completes work. Verify health/arrival before retrying: retries may duplicate context.

## Connect an OpenCode recipient

Use an existing native session in the same workspace. Confirm the endpoint and native session ID with its host.

1. Start `opencode serve --hostname 127.0.0.1 --port 4096` and create or reuse an idle session.
2. Bind your database identity to that receiver:

   ```sh
   scripts/agents-communication attach '{"transport":"opencode","endpoint":"http://127.0.0.1:4096","vendorSession":"OPENCODE_SESSION_ID"}' \
     --workspace /absolute/project --database /absolute/shared/communication.sqlite \
     --session EXACT_DB_AGENT_ID
   ```

3. Run one supervised `listen` process with the same workspace, database, and database identity.

The endpoint must be literal-loopback HTTP with an explicit port, without a path or credentials.
If authentication is enabled, supply only the listener environment with `OPENCODE_SERVER_PASSWORD`, optional `OPENCODE_SERVER_USERNAME`, and `OCTOCODE_OPENCODE_AUTH_ENDPOINT`.
Set the last variable to the exact attached endpoint string. Credentials stay outside the database.

Preflight checks native session identity, canonical directory, and idle status before staging. Busy sessions defer.
Passive mail uses `noReply:true`; action uses `prompt_async`. Submission does not acknowledge handling.
[Service receipts](SERVICE_PROTOCOL.md#identity-and-capabilities) and [optional edit admission](HOST_LEASE_GUARDS.md#opencode-opt-in) describe their separate contracts.

## Install messaging hooks

Use native delivery when an existing host binding supports it. For a host with local lifecycle hooks, `host-config` generates a raw messaging fallback for Claude, Codex, Grok, or Cursor. Skill installation and hook installation are separate steps. Keep the installed npm package or extracted runtime archive at a stable path for host hooks.

1. Choose the recipient’s actual worktree and one database path shared by all participants. Generate a preview from the installed CLI (replace paths and `claude` with the actual host):

   ```sh
   /absolute/runtime/scripts/agents-communication host-config --vendor claude \
     --workspace /absolute/project --database /absolute/shared/communication.sqlite \
     > communication-hooks.preview.json
   ```

2. Inspect the JSON and stderr capability receipt. The command prints settings only; it installs nothing, joins no identity, and creates no DB. Merge each generated event array into the existing `hooks` object at the host location below. Preserve unrelated keys and handlers; avoid duplicate entries. For a new file, the preview is the complete JSON config. Keep previews outside discovered hook directories until ready to enable them.

   | Host | Project location | Enable and verify |
   | --- | --- | --- |
   | Claude Code | `.claude/settings.local.json` (or shared `.claude/settings.json`) | Restart/resume; inspect `/hooks`; hooks must be enabled. For an isolated run use `claude --settings /absolute/preview.json`. |
   | Codex | `.codex/hooks.json` beside the active project config | Review and trust the definition in `/hooks`; project config must be trusted and the `hooks` feature enabled. |
   | Grok Build | `.grok/hooks/communication.json` | From the Git project root, run `grok inspect --json`; verify `projectRoot`, `projectTrusted`, and loaded hooks. |
   | Cursor | `.cursor/hooks.json` | Merge `version:1` and generated hooks; restart and inspect host hook diagnostics. |

3. Start a host session. `SessionStart` registers/reuses the identity. Claude/Codex/Cursor receive initial binding context; Grok emits it on its first supported post-tool event. Wait for the hook to finish, then run `peers` with the same workspace/DB to find the exact recipient ID: Grok ACP session creation can return before its async `SessionStart` hook finishes. The host's binding context includes its CLI flags. Keep heartbeat/listener presence when the host stays idle for more than 60 seconds.
4. In an authorized isolated check, send a small direct message from a second participant. Have the recipient run a harmless tool: the post-tool event should inject it once. Recover it through `inbox` if needed, then `complete` its exact ID. `health` can expose storage/delivery trouble; it cannot prove the model read the text. `SessionEnd` leaves the identity and releases its leases.

All previews use ten-second timeouts and absolute interpreter/script paths. Regenerate after moving the installed folder or changing Python. Hook configuration requires the host's normal trust/approval flow; preview generation grants no approval. These messaging hooks install no edit guard. Claude can add the separate [lease guard](HOST_LEASE_GUARDS.md#claude-opt-in); Codex and Grok have none bundled.

Claude/Codex child events can carry the parent's session ID. The messaging hook ignores events with `agent_id` so a child cannot consume its parent's mailbox. Give independently participating subagents their own identity and bound MCP/CLI tools. A native-bound participant keeps lifecycle updates but receives no second hook delivery, except a Claude receiver whose reported permission mode or `crossSessionInbound` setting holds or refuses socket messages: its hook delivers instead (see [Claude inbound](SERVICE_PROTOCOL.md#unified-adapter-protocol)).

**When wiring a host adapter**, use these bundled `scripts/` files; keep the complete `scripts/` folder and Python available:
- `inbox-hook` (`.ps1`): run from a host context event; inject stdout into agent context, empty means no new messages.
- `pi-inbox.mjs`: Pi bridge (`registerPiInbox`); loads `pi-extension.mjs` for bound tools and adds edit guards for `editing`.
- `hooks/claude-lease-guard.mjs --config --binary … --workspace … --database … --session … --host-session …` prints Claude `PreToolUse` Write/Edit/MultiEdit/NotebookEdit hooks to merge.
- `hooks/opencode-lease-guard.mjs`: `createOpenCodeLeaseGuard({binary,workspace,database,sessions})` returns an OpenCode plugin guarding write/edit.
- `hooks/lease-check.mjs`: shared `check_write` admission for the guards; never run it on its own.

Linked Git worktrees communicate through the same local DB and canonical Git common-directory scope.
Each agent keeps its actual workspace; native endpoints and file leases use that checkout.
Independent clones and unrelated projects remain isolated. SQLite stores state; evidence documents remain separate files. `db export` excludes files; back them up separately. Codex native delivery is supported, but this bundle has no Codex edit guard.

For recovery and health, use [operations](OPERATIONS.md); for delivery semantics use [the service protocol](SERVICE_PROTOCOL.md).


Managed workers receive the operating guide once. Profiles without `lock` or `lock_many` omit the editing section and flowchart.
Their binding identifies the actual workspace, repository scope, branch, available tools, lifecycle owner, and instruction version/hash.
When no actionable work remains, the worker ends its turn. The host waits for mail and starts the next turn; model sleep or polling does not maintain delivery.
