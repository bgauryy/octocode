# Host setup and administration

Load when configuring identity, delivery, tool profiles, edit guards or storage. Bundle setup: [installation](INSTALLATION.md); coordination: [the operating guide](../OPERATING.md); examples: [commands](COMMANDS.md); record types: [records](RECORDS.md). Before an unfamiliar call, read `<command> --help` or `schema <command>`; don't guess JSON fields.

## Setup

MCP runs over stdio with one bound process per agent: JSON-RPC on stdout, diagnostics on stderr. For npm, use the [README MCP example](../README.md#connect-through-mcp). The `/cli` route needs the explicit `mcp` command:

```sh
npx -y @octocodeai/octocode-agents-communication /cli mcp --managed --tools messaging --name <agent> --vendor <host> \
  --workspace <repo> --database <db>
```

`--managed` owns the lifecycle ([lifecycle owners](INTERFACES.md#own-the-lifecycle)); read `inbox` at task boundaries, since it wakes no idle host. With a host-owned identity and delivery owner, use `mcp --session <id>` instead. Without MCP, reuse a binding or `join`, then `attach` real host endpoints and IDs, with one delivery owner per identity. Presence and renewal by owner: [workflow](WORKFLOW.md#presence-and-lease-expiry). `OCTOCODE_COMMUNICATION_HEARTBEAT_MS` (50–60000) shortens managed loop periods for tests only; presence and lease TTLs never change.

`--tools` takes `messaging`, `review`, `editing` or a comma-separated list. Enable a guard for editing ([lease guards](HOST_LEASE_GUARDS.md)); guards cover only their structured operations, never shell or OS writes. `host-config --help` previews Claude, Codex, Grok and Cursor hooks; Pi uses its extension. `completion-check` allows one recovery turn and never completes work. Verify health and arrival before retrying: retries can duplicate context.

Managed workers receive the operating guide once (profiles without `lock` or `lock_many` omit its editing section). Their binding names the workspace, repository scope, branch, tools, lifecycle owner and instruction version. When no actionable work remains, the worker ends its turn; the host waits for mail and starts the next one, so model sleep or polling never maintains delivery.

## Connect an OpenCode recipient

Use an existing native session in the same workspace. Confirm the endpoint and native session ID with its host.

1. Start `opencode serve --hostname 127.0.0.1 --port 4096` and create or reuse an idle session.
2. Bind your database identity to that receiver:

   ```sh
   npx -y @octocodeai/octocode-agents-communication /cli attach '{"transport":"opencode","endpoint":"http://127.0.0.1:4096","vendorSession":"OPENCODE_SESSION_ID"}' \
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

Use native delivery when an existing host binding and its installed version have been verified. Codex documents its app-server methods; Pi documents its extension API. Claude's messaging socket and Grok's leader-socket envelope are version-specific local transports. For a host with documented local lifecycle hooks, `host-config` generates a raw messaging route for Claude, Codex, Grok, or Cursor. Skill installation and hook installation are separate steps. Hook commands name the installed entry by absolute path, so keep the installed npm package at a stable path.

1. Choose the recipient’s actual worktree and one database path shared by all participants. Generate a preview from the installed CLI (replace paths and `claude` with the actual host):

   ```sh
   npx -y @octocodeai/octocode-agents-communication /cli host-config --vendor claude \
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

All previews use ten-second timeouts and absolute `node` and entry paths. Regenerate after moving the installed package or changing Node.js. Hook configuration requires the host's normal trust/approval flow; preview generation grants no approval. These messaging hooks install no edit guard. Claude can add the separate [lease guard](HOST_LEASE_GUARDS.md#claude-opt-in); Codex and Grok have none bundled.

Claude/Codex child events can carry the parent's session ID. The messaging hook ignores events with `agent_id` so a child cannot consume its parent's mailbox. Give independently participating subagents their own identity and bound MCP/CLI tools. A native-bound participant keeps lifecycle updates but receives no second hook delivery, except a Claude receiver whose reported permission mode or `crossSessionInbound` setting holds or refuses socket messages: its hook delivers instead (see [Claude inbound](SERVICE_PROTOCOL.md#unified-adapter-protocol)).

**When wiring a host adapter**, use the package's `dist/hosts/` modules, which the package exports as `@octocodeai/octocode-agents-communication/hosts/<name>`; the `binary` option of each is the package's `dist/communication.js` entry (a `.js` file, run with `node`):
- Context hook: run `npx -y @octocodeai/octocode-agents-communication /cli hook --session ID --workspace … --database …` from a host context event; inject stdout into agent context, empty means no new messages.
- `pi-inbox`: Pi bridge (`registerPiInbox`); loads `pi-extension` for bound tools and adds edit guards for `editing`.
- `pi-extension`: bound communication tools for Pi (`registerBoundTools`, or the default export reading `OCTOCODE_COMMUNICATION_BINDING`). `run --vendor pi` starts its worker with `--extension <package>/dist/hosts/pi-extension.js` (and `--no-extensions`), so the worker needs no other setup.
- `claude-lease-guard` is an entry script, run with `node` rather than imported: `node dist/hosts/claude-lease-guard.js --config --binary … --workspace … --database … --session … --host-session …` prints Claude `PreToolUse` Write/Edit/MultiEdit/NotebookEdit hooks to merge.
- `opencode-lease-guard`: `createOpenCodeLeaseGuard({binary,workspace,database,sessions})` returns an OpenCode plugin guarding write/edit.
- `lease-check`: shared `check_write` admission for the guards; never run it on its own.

`db export` excludes evidence documents; back them up separately ([operations](OPERATIONS.md#retention)). Delivery semantics: [service protocol](SERVICE_PROTOCOL.md).

## MCP initialization and provider hooks

The MCP server gates `init` on its connection binding, not files on disk. Pass `--session` when a hook, extension or native adapter already owns the communication identity. Otherwise the unbound server exposes only `init`; it creates a managed identity and then signals a tool-list change. An exact current `vendorSession` reuses one existing live identity without taking ownership; an ambiguous or expired match is refused. `init` never installs hooks. See [the MCP contract](INTERFACES.md#use-mcp-without-the-skill).

| Provider | Package integration | What proves the current binding | Delivery while idle |
| --- | --- | --- | --- |
| Claude Code | Generated session/prompt/post-tool/end hooks; optional native socket | Validated current hook event or exact native attachment | Hooks wait for events; native wake depends on host policy |
| Codex | Generated session/prompt/post-tool/end hooks; optional app-server attachment | Validated current hook event or loaded native thread | Hooks wait for events; native delivery can request a turn |
| Grok Build | Generated project hooks; optional leader socket | Trusted, loaded hooks and current event, or exact ACP attachment | Hooks wait for events; native socket supports action delivery |
| Cursor | Generated camelCase hooks using the conversation ID | Current event with the correct workspace and conversation | Hooks wait for events; no package native wake adapter |
| Pi | Package extension and inbox bridge | Successful extension session binding | Extension can trigger an action-mail turn |
| OpenCode | Server API attachment and listener; optional separate edit guard | Exact live native session and listener binding | Native action delivery can request a turn |

A configuration file proves only that a file exists. Providers may merge user, project, managed or plugin settings, require project trust, disable hooks, or load extensions separately. No received event means the hook state is unknown. MCP alone cannot establish the host's effective hook state or wake idle agents. An exact binding is the usable signal; do not infer an identity from peer names or copy another agent's session.

Provider contracts: [Claude hooks](https://code.claude.com/docs/en/hooks), [Codex hooks](https://developers.openai.com/codex/hooks/), [Cursor hooks](https://cursor.com/docs/hooks), [Grok hooks](https://github.com/xai-org/grok-build/blob/main/crates/codegen/xai-grok-pager/docs/user-guide/10-hooks.md), [Pi extensions](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/extensions.md), and [OpenCode server API](https://opencode.ai/docs/server/). Package tests cover emitted configuration, identity and delivery contracts with fixtures; they do not prove a particular user's live host configuration.
