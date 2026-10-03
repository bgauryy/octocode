# Host setup and administration

Load when configuring identity, delivery, tool profiles, edit guards, or storage. Ordinary workers use [the skill workflow](../../SKILL.md).

`<command> '<json>' --workspace <repo> --database <db> --session <id>`; `-` reads stdin. Before unfamiliar CLI calls, read `<command> --help` or `schema <command>`; don't guess JSON fields. Bare `schema` includes full protocol/SQL; workers expose a subset.

| Purpose | Commands |
| --- | --- |
| Identity | `join`, `heartbeat`, `resume`, `leave`, `set_status` |
| Discovery | `peers`, `activity`, `context` |
| Messages | `send_message`, `notify_all`, `subscribe`, `inbox`, `inbox wait`, `complete` |
| Documents | `share_document`, `read_document` |
| Leases | `locks`, `lock`, `lock_many`, `renew`, `unlock`, `check_paths`, `check_write` |
| Delivery | `attach`, `listen`, `dispatch`, `hook`, `confirm_delivery`, `retry_delivery` |
| Hosts | `mcp`, `run`, `host-hook`, `host-config`, `completion-check`, `record_usage` |
| Local dashboard | `view` |
| State | `entity get`, `entity list`, `entity set`, `health` |
| Database | `db info`, `db protocol`, `db export`, `db retention`, `db compact`, `prune` |
| Help | `skill`, `schema`, `schema tools` |
Entities: `schema entity <name>`.

## Setup
MCP uses stdio: the host launches the command below; JSON-RPC on stdout, diagnostics on stderr. One bound process per agent.
For standalone MCP, launch `scripts/agents-communication mcp --managed --tools messaging --name <agent> --vendor <host> --workspace <repo> --database <db>`: it owns identity, presence, live lease renewal and exit cleanup; read `inbox` at task boundaries. It does not wake idle hosts. With a host-owned identity/delivery owner, use plain `mcp --session <id>`.
Reuse bindings; else `join`, `attach` real host endpoints/IDs. Use one delivery owner. `listen` maintains presence and native delivery; otherwise heartbeat every 15s (60s presence). Plain heartbeats cannot revive identity or renew leases.

Managed MCP and `run` workers use `heartbeat {"renewLeases":true}` every 15 seconds. The Pi inbox adapter checks periodically (roughly 15–25 seconds with idle backoff): live owned leases extend to at least 60 seconds from that tick, longer leases stay unchanged, and expired leases are never revived. They advertise managed renewal to the worker. Use the default 60-second lease TTL; a custom TTL shorter than a host tick can expire before renewal and must be reacquired. A plain listener or externally bound MCP does not promise lease renewal; its owner must arrange it explicitly. `leave` on completion; `run` only when requested.
Choose `--tools messaging` for coordination, `review` for shared evidence, or `editing` for leases too; explicit comma lists also work. Report supported/configured edit guards; enable them for editing. Guards cover only reported operations, never arbitrary shell/OS writes.
`host-config --help` configures hooks/Pi. Context hooks cannot wake idle hosts. `completion-check` allows one recovery turn, never automatically completes work. Verify health/arrival before retrying: retries may duplicate context.

**When wiring a host adapter**, use these bundled `scripts/` files; keep the complete `scripts/` folder and Python available:
- `inbox-hook` (`.ps1`): run from a host context event; inject stdout into agent context, empty means no new messages.
- `pi-inbox.mjs`: Pi bridge (`registerPiInbox`); loads `pi-extension.mjs` for bound tools and adds edit guards for `editing`.
- `hooks/claude-lease-guard.mjs --config --binary … --workspace … --database … --session … --host-session …` prints Claude `PreToolUse` Write/Edit hooks to merge.
- `hooks/opencode-lease-guard.mjs`: `createOpenCodeLeaseGuard({binary,workspace,database,sessions})` returns an OpenCode plugin guarding write/edit.
- `hooks/lease-check.mjs`: shared `check_write` admission for the guards; never run it on its own.

Use a local same-machine DB, not a cross-machine network share. SQLite stores state; documents stay in workspace files. `db export` excludes files; back them up separately.

For recovery and health, use [operations](OPERATIONS.md); for delivery semantics use [the service protocol](SERVICE_PROTOCOL.md).
