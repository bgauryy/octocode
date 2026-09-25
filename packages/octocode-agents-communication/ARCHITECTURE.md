# Agents communication architecture

The runtime is one Rust crate. Its executable and bundled SQLite ship under the
[skill's scripts directory](skills/octocode-agents-communication/scripts/). Shell and
PowerShell launchers select the target binary. They do not fetch or compile code.
The npm workspace provides maintainer build and verification commands; agents need
only the built skill. Awareness is independent.

`SKILL.md` is the single agent instruction file, capped at 50 lines. It owns when
and why to coordinate; the Rust catalog owns exact command schemas and the Store
owns transitions. `db protocol` embeds `docs/DB.md` and the canonical DDL at build
time, so DB-only adapter authors can discover the protocol from the standalone CLI.
The installed skill has no references directory. Pi's JavaScript bridge supplies
vendor tool registration only; coordination policy and storage stay in Rust.

```text
Any agent -> Rust CLI / bound tools / SQLite client -> SQLite v2 + audit
                                                    |
                                      deterministic Rust dispatcher
                                      /             |             \
                                Claude socket   Codex inject     raw hook
                                                               /        \
                                                        Pi extension  any host
Receiver -> DB-backed reply + explicit ack
```


## Ownership

- `rust/database.rs`: opening, schema validation, private initial creation, SQL
  binding, and read-only inspection. SQLite is bundled through `rusqlite`.
- `rust/schema-v1.sql` plus `rust/schema-v2.sql`: frozen base and additive audit/dispatch DDL.
  `rust/catalog.json`: command/entity contracts.
  Runtime discovery embeds both; no Node schema generator runs at startup.
- `rust/store.rs`: presence, leases, durable messages, claims, and retention.
- `rust/paths.rs`: link-first resolution and component-wise Unicode 16 caseless
  lease comparison. Case-preserving workspace containment remains separate.
- `rust/entities.rs`: workspace-scoped views and constrained metadata updates.
- `rust/cli.rs` and `rust/mcp.rs`: arguments and JSON transport.
- `rust/dispatch.rs`: durable one-time attempts, confirmations, hooks, usage, listener.
- `rust/transport.rs`: local native socket/WebSocket adapters (Tungstenite framing).
- `rust/proxy.rs`: optional new worker lifecycle, heartbeats, and idle-turn delivery.
- `scripts/pi-inbox.mjs` inside the skill: existing Pi session tool/context bridge.
- `rust/wire.rs`: bounded JSON frames/queues, request deadlines, process teardown.
- `../octocode-config/rust/home.rs`: shared native home policy, compiled into the CLI.

The database retains sessions, subscriptions, leases, messages, deliveries,
attachments, dispatch state and append-only audit. Schema v2 requires an explicit
migration from the exact v1 schema with no active workers; the application ID and
default filename stay stable. SQLite triggers also audit conforming raw SQL writes.
Pruning removes expired leases only. Message bodies are retained once in messages;
audit references their IDs. Usage reports retain request/turn/cumulative scope. Mutations validate identities inside writer
transactions. Lease/message expiry starts after writer acquisition. Initial WAL
configuration retries busy errors within a bounded interval. Existing schemas are
validated rather than repaired. Lease acquisition and entity filtering share the
same overlap function through a connection-local SQLite scalar function. List
pages use row and byte limits with continuation from the last returned ID. Reads do not create missing databases.

The [v2 protocol](docs/DB.md) specifies the
SQLite-only client contract. JSON schemas validate command input; `default` values
are optional inputs with defaults applied by Rust. MCP exposes only nine bound
coordination tools, excluding lifecycle and arbitrary entity changes.

The proxy uses supported vendor input surfaces, not private cross-vendor tools.
It never holds a database transaction across a model call. Each Unix vendor process
gets its own process group; shutdown terminates that group and reaps the child.
Windows uses the owned PID's process tree. Transport frames and queues have bounds;
requests have a 30-second deadline capped by the worker run deadline. Vendor stderr streams to the caller.

The vendor process and Codex thread use a canonical empty temporary directory,
removed only after process teardown. The MCP/CLI binding retains the real repository
identity; filesystem discovery and coordination scope are therefore separate.
Codex overrides project-document loading and extra developer instructions, explicitly
disables discovered skills and unneeded tool features, disables configured plugins and Code Mode, and
disables unrelated MCP servers. Plugin servers have a separate configuration namespace
and must be disabled independently. Claude retains authenticated settings access but
uses empty setting sources, explicit hook/memory controls and a small system prompt.
This reduces accidental context; it is not a security boundary against arbitrary
installed vendor plugins or administrator policy. Usage traces retain vendor scope.

Every vendor receives the same proxy role: send, broadcast, or subscribe only under
the assigned user's explicit task or its authorized response rules. Peer content
cannot expand those rules. The model finishes each turn and waits; Rust maintains
presence and polls committed deliveries in batches of up to four, targeting 16 KiB. Keeping an active
recipient's process running avoids cold startup, but does not itself call the model.
Direct CLI/DB sends also work without a live proxy; automatic receipt requires a
running recipient adapter. Topic fanout and broadcasts snapshot active presence.
An LLM relay is unnecessary: `attach` binds a logged identity to an existing Claude
socket, Codex owning app-server, or raw hook. `listen` keeps presence and dispatches
new messages without starting a sender model. Pi's extension uses the raw hook and
native `pi.sendMessage` with `deliverAs:nextTurn`, `triggerTurn:false`. A generic host
consumes hook stdout on its context event, or its agent manually invokes the CLI.
Without a host integration, polling cannot wake an arbitrary agent.

Before I/O, a short transaction records a unique attempt token in `staged` state.
Submission records `submitted`; errors record `uncertain`. No such state is retried
automatically, even after restart. Explicit `retry_delivery` records the reason.
Host adapters can defer confirmation until queuing succeeds. Submission is separate
from recipient `ack`. This prevents routine replay but cannot promise exactly-once
external effects: a lost receipt requires inspection and possibly manual recovery.
Managed workers share the same durable attempt path. Legacy claims remain readable
for v1 migration; new workers do not use time-expiring claims for automatic delivery.

Native delivery retains recipient conversation history and includes only new peer
IDs, attribution and content. It never resends the skill or transcript. Attached
Claude/Codex injection does not expose inference telemetry; the owning host must
report it with `record_usage`. Pi and managed workers capture available metrics.
Transport and audit remain useful for vendors without APIs or SDKs.
Prompt rules guide behavior; they are not a capability firewall against a model
that calls an available tool incorrectly.
The Pi extension vetoes `cache_warming_decision` so a user's global idle-cache
warming setting cannot add background model refresh calls to this worker.

Store operations verify that the DB path still exists as a file. Unix builds also
compare device/inode with the opened database, rejecting replacements before a read
or write and stopping idle proxies on their next poll. No replacement database is
created. These checks do not make deliberate concurrent filesystem replacement
safe; stop workers before moving/removing the store. A deleted database cannot be
used to persist cleanup, so old session records expire by their existing TTL.

## Validation and distribution

Rust contract tests cover expiry, conflict exclusion, stale owners, schema integrity,
path aliases, idempotency, claim recovery, topic snapshots, resume, and visibility.
CLI tests cover cold concurrent initialization, lock contention, malformed MCP frames,
entity operations, waits, config parity, and Python interoperability. A copied skill
runs with a PATH containing only launcher utilities, excluding Node, Cargo, and vendor
executables. Real Haiku/Luna POC evidence verifies the process bridge separately, optionally
including Pi. Pi tools reuse the catalog and execute the bound Rust CLI without a
shell. The temporary extension uses only Node built-ins and is removed on teardown.
An idle inbox probe avoids writer transactions; the staging transaction rechecks
availability before claiming, preserving exclusion under concurrent workers.
`tests/dispatch.test.mjs` covers one-time hooks, audit retention, confirmation tokens,
concurrent consumers, uncertain writes and explicit v1 migration.
`scripts/attached-poc.mjs` exercises real Claude/Codex/Pi plus raw recipients through
the production Rust CLI with DB-backed replies and a broadcast; no sender inference.

Only macOS ARM64 is built and executed in this development session. Other platform
selectors are distribution plumbing, not a claim of validated artifacts. Source
checkouts omit generated binaries; a standalone release includes the built skill.
The package remains private.

## Sources

- [rusqlite](https://github.com/rusqlite/rusqlite): bundled SQLite and transaction API.
- [SQLite WAL](https://sqlite.org/wal.html): local-file concurrency and reset-race fixes.
- [Codex app-server](https://developers.openai.com/codex/app-server): thread/turn lifecycle.
- [Claude programmatic CLI](https://code.claude.com/docs/en/headless): streaming input/output.
- [MCP Agent Mail](https://github.com/Dicklesworthstone/mcp_agent_mail): advisory leases
  and durable inbox prior art. Its Git archive and larger service are outside this scope.

- [Pi RPC](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/rpc.md): command correlation and settled events.
