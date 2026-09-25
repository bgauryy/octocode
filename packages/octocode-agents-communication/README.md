# Octocode agents communication

`@octocodeai/octocode-agents-communication` is a Rust agentic CLI distributed inside
its [communication skill](skills/octocode-agents-communication/SKILL.md). Agents use
the skill's launcher; the built skill contains the executable and SQLite. Node, npm,
Cargo, and a separate CLI installation are unnecessary for coordination. Managed
workers use their vendor CLI and its runtime (Pi requires Node).

It coordinates session identity, advisory path leases, direct messages, and exact
topic subscriptions. `notify_all` broadcasts to all other active peers in the same
workspace without subscriptions. SQLite v2 stores every participating identity,
message, delivery and audit event. Existing v1 stores require explicit `db migrate`
after stopping workers. Awareness stays independent and unchanged.

## Use the skill

Install or copy the **built** `skills/octocode-agents-communication/` folder. From any
project, invoke the launcher using its installed absolute path:

```sh
/absolute/skill/scripts/agents-communication --help
/absolute/skill/scripts/agents-communication send_message --help
/absolute/skill/scripts/agents-communication db info
/absolute/skill/scripts/agents-communication schema entities
/absolute/skill/scripts/agents-communication join \
  '{"name":"reviewer","vendor":"other"}' --workspace /absolute/project
```

Use `scripts/agents-communication.ps1` on Windows. The launcher chooses a bundled
platform executable under `scripts/bin/<Rust target>/`. It does not download or build
code on an agent's behalf. Source-only GitHub checkouts need the maintainer build;
release skill bundles must include binaries for their advertised platforms. This
package is private and unpublished. The built and tested artifact in this checkout
is macOS ARM64; other target selectors are present but not validated here.

The CLI's help links to the skill's GitHub source, and `skill` returns its embedded
instructions offline. The GitHub links resolve after the package is pushed. The npm
name remains an optional distribution identity; using the skill requires no npm step.
`SKILL.md` is the only instruction file (49 lines): it explains peer awareness,
intent and reasons, reservations before edits/renames/deletions, and verified handoffs.
Detailed contracts live in CLI help; `db protocol` returns the embedded SQLite
protocol and DDL offline, without opening a database or requiring another document.

## Coordinate

Every participant uses the same database and canonical workspace. The default is
`<Octocode home>/agents-communication/v1.sqlite`; `--database` overrides it. `db info`
reports the resolved location and compatibility without creating storage. Worktrees
have separate workspace identities. Use a local filesystem.

`--help` gives a compact workflow and command list. `<command> --help` or
`schema <command>` gives just that command’s usage and input schema. Bare `schema`
returns the complete catalog and DDL; `schema entity <name>` describes fields, filters,
and allowed edits for sessions, leases, messages, deliveries, subscriptions,
attachments, dispatch receipts and audit events.

```sh
/absolute/skill/scripts/agents-communication entity get session SESSION_ID \
  --session SESSION_ID --workspace /absolute/project
/absolute/skill/scripts/agents-communication entity set subscriptions SESSION_ID \
  '{"topics":["review"]}' --session SESSION_ID --workspace /absolute/project
/absolute/skill/scripts/agents-communication send_message \
  '{"to":"RECIPIENT_ID","body":"Review ready","key":"review-1"}' \
  --session SESSION_ID --workspace /absolute/project
/absolute/skill/scripts/agents-communication notify_all \
  '{"body":"Review complete","key":"review-complete-1"}' \
  --session SESSION_ID --workspace /absolute/project
/absolute/skill/scripts/agents-communication lock '{"path":"src/api","kind":"tree"}' \
  --session SESSION_ID --workspace /absolute/project
```

Manual sessions send `heartbeat` about every 15 seconds and `leave` when finished.
Presence expires after 60 seconds. `resume` with the same vendor restores an expired
identity and pending messages, discarding old leases. `inbox wait` polls for up to
60 seconds; another process must maintain presence. Reads never acknowledge; use
`ack` after processing. Pages target 256 KiB and at most 100 rows; continue through `next` using `after`.

Leases require cooperative writers; they do not prevent arbitrary OS writes or SQL.
Keep the acquisition ID, renew before expiry, and stop editing on failed renewal.
Lease names use Unicode 16 canonical caseless comparison on every filesystem:
case/normalization aliases conflict before creation, including on case-sensitive
volumes. Stored and accessed paths retain their spelling. Resolve symlinks before
parent traversal. This conservative rule keeps native and DB-only clients aligned
without filesystem probes or placeholder files. Stop old workers and release or
expire their leases before upgrading all participants to these rules.
Messages remain in the audit store after expiry and use sender-scoped retry keys.
Delivery eligibility expires; native/hook delivery never automatically replays an
attempt after a crash. Inspect uncertain attempts before an explicit retry. Topic sends snapshot active subscribers. The [SQLite protocol](docs/DB.md)
defines transactions and visibility; its Python example needs no vendor binaries (Python 3.14 / Unicode 16 for path leases).
The former JavaScript client has been removed. Programmatic consumers can use the
Rust `Store`, CLI JSON, or the SQLite protocol.

## Deliver into existing agents

No sender/proxy model is needed. Every sender and recipient first joins the same
DB/workspace. Use that identity's `--session`, `--workspace` and optional `--database`
on each command. All peer messages and replies use `send_message`/`notify_all` so
routing and audit remain in our DB.

```sh
# Any vendor, including one without an SDK:
/absolute/skill/scripts/agents-communication attach '{"transport":"raw"}' --session ID --workspace /project
/absolute/skill/scripts/inbox-hook '{"format":"text"}' --session ID --workspace /project

# Existing Claude receiver: explicitly supply its exported inbox and session ID.
/absolute/skill/scripts/agents-communication attach \
  '{"transport":"claude","endpoint":"/absolute/inbox.sock","vendorSession":"CLAUDE_SESSION"}' \
  --session ID --workspace /project

# Existing Codex thread: connect to the server that owns that loaded thread.
/absolute/skill/scripts/agents-communication attach \
  '{"transport":"codex","endpoint":"ws://127.0.0.1:4500","vendorSession":"CODEX_THREAD"}' \
  --session ID --workspace /project
/absolute/skill/scripts/agents-communication listen --session ID --workspace /project

# Pi: native extension joins/logs identity, registers tools, polls and queues context.
pi --extension /absolute/skill/scripts/pi-inbox.mjs
```

`listen` is a lightweight Rust process; `dispatch {}` performs one batch instead.
Codex injection is passive until its host starts a turn. Claude may respond
immediately according to inbound policy. Pi queues new context for its next turn
without idle model calls. Its optional `OCTOCODE_COMMUNICATION_BINDING` JSON supplies
`workspace`, `database`, `session` and/or `binary`; omitted identity is created
with vendor `pi`. An existing binding must name a Pi identity and resumes if expired.
Read the single skill once in the recipient; Pi loads tools but does not silently
install skills or change host settings.

For a generic host, register `scripts/inbox-hook` on its context-injection event.
Text goes into additional context; JSON adapters use `format:"json"`; a Claude
`UserPromptSubmit` hook uses `format:"claude"`. SDK adapters can defer the receipt
until their queue accepts input using `deferConfirm` and `confirm_delivery` (see
command help). Hook output is peer data, never developer/system authority. If a
host offers no injection hook, the agent reads the CLI hook or inbox itself. A DB
row cannot wake an arbitrary agent, and these adapters do not attach to unrelated
desktop sessions or write vendor transcripts.

Each delivery is durably staged before I/O. It is offered once; acknowledgement
means handled, not merely written to a socket. Crashes/uncertain writes remain
inspectable via `entity list dispatch`; `retry_delivery` requires a reason and
can duplicate a previously received message. `inbox` is the explicit recovery view.
`prune` removes expired leases while preserving identities, messages and audit.
`entity list audit` shows transitions and available `record_usage` metrics. Native
injection cannot observe the owner's token use; report it from the host when known.

[Production adapter validation](docs/VENDOR_MESSAGES.md#production-db-first-adapters)
uses real existing Claude, Codex and Pi receivers plus a raw CLI recipient. Run
`node scripts/attached-poc.mjs` from this package after building; it checks DB-backed
request/reply, one-time hooks, passive injection and broadcast acknowledgements.

## Managed vendor workers

```sh
/absolute/skill/scripts/agents-communication run \
  --vendor codex --model gpt-6-luna --name luna \
  --workspace /absolute/project --prompt 'Coordinate this review with peers.'
/absolute/skill/scripts/agents-communication run \
  --vendor claude --model haiku --name haiku \
  --workspace /absolute/project --prompt 'Coordinate this review with peers.'
/absolute/skill/scripts/agents-communication run \
  --vendor pi --model PROVIDER/MODEL --name pi-reviewer \
  --workspace /absolute/project --prompt 'Coordinate this review with peers.'
```

These optional adapters require installed, authenticated Codex, Claude, or Pi CLIs. The
Rust proxy supplies the exact embedded skill to each worker, owns its process,
maintains presence, and delivers messages at idle
turns through Codex app-server, Claude streaming input, or Pi RPC. Codex and Claude
receive nine bound MCP tools; Pi registers the same tools through a temporary
native extension that calls the Rust CLI. Pi waits for `agent_settled`, including
automatic retries, before another inbox delivery. Use an explicit Pi `provider/model`
from `pi --list-models`; ambiguous aliases can select an unauthenticated provider. Cross-vendor `send_message` and `notify_all` are this package's bound tools. Each
vendor invokes them through its supported tool interface; the shared SQLite store
owns routing, recipient snapshots, claims and acknowledgements. Claude's built-in
`SendMessage` routes Claude sessions; Pi's `pi.sendMessage()` adds session context.
Those APIs do not provide the shared cross-vendor database contract. See
[the live communication reviews](docs/MESH_REVIEW.md) for the verified routes and limits.
Claude and Pi expose communication tools; Codex runs read-only. Pi disables
automatic extension, skill, prompt-template, and context-file discovery. Existing vendor settings
stay intact. The managed adapter creates a new recipient; use `attach` for reachable existing
receivers. Vendor conversation history restoration is outside managed `run`. `--duration-ms` bounds a worker run,
including protocol startup waits. `--trace` includes actual communication tool
results, which can contain message text; enable it only when those logs are wanted.
Idle workers read SQLite without taking a writer lock when no delivery is available.
No model call is made merely to poll or route a message.

Managed vendors start in a disposable empty directory; their bound tools still
coordinate the requested repository. Codex disables repository instructions,
configured plugins, Code Mode and unrelated MCP servers; Claude uses an explicit small system prompt and disables hooks and
auto memory without switching away from existing authentication. The skill, tool
schemas, task and subsequent conversation still consume context. `--trace` emits
vendor-reported `usage` events with `thread`, `result` or `message` scope; DB audit
normalizes these to `cumulative`, `turn` and `request`. Preserve
those scopes and cache counters rather than summing overlapping reports.

Run `node scripts/context-poc.mjs` from this package for the live canary/context and
peer-deletion-request audit. `COMMUNICATION_EXPECT_ISOLATED=1` requires that neither
vendor sees the repository marker; `COMMUNICATION_PI_MODEL` also includes Pi.
`COMMUNICATION_OUTPUT` selects the evidence file. The [context review](docs/CONTEXT_REVIEW.md)
records actual usage, rejected approaches and test limits.

## Build and distribute

From the monorepo root:

```sh
yarn workspace @octocodeai/octocode-agents-communication build
yarn workspace @octocodeai/octocode-agents-communication build:release
yarn workspace @octocodeai/octocode-agents-communication pack:skill
```

Maintainer builds need Rust, a C compiler for bundled SQLite, and Node for the build
script. Cargo uses the committed lockfile. The script copies the executable into the
skill atomically and writes `SHA256SUMS`. Rust source is under `rust/`; the shared
native home policy is owned by `packages/octocode-config/rust/home.rs`.

Set `CARGO_BUILD_TARGET` to build another target using an appropriate compiler and
linker. Build each advertised target before packaging; the script preserves other
platform binaries already present. `pack:skill` checks binary checksums and creates a
standalone skill archive under `out/`; packaging uses the system `tar`. Generated binaries are ignored by Git. A release
must distribute the built skill folder or archive, not only its tracked source files.

## Verify

```sh
COMMUNICATION_PYTHON=/absolute/python3.14-with-sqlite-3.51.3-or-later \
  yarn workspace @octocodeai/octocode-agents-communication verify
yarn workspace @octocodeai/octocode-agents-communication poc
COMMUNICATION_PI_MODEL=PROVIDER/MODEL \
  yarn workspace @octocodeai/octocode-agents-communication poc --pi
```

For the full nine-worker matrix (three processes per vendor), set both
`COMMUNICATION_PYTHON` and `COMMUNICATION_PI_MODEL` and run
`yarn workspace @octocodeai/octocode-agents-communication poc:mesh`. It verifies all
72 directed worker pairs with replies, each worker's `notify_all`, Python DB-only
interop, and lock conflicts plus acquire/renew/release. Completion is decided by
stored messages, acknowledgements and tool receipts, not model declarations.
Set `COMMUNICATION_WORKERS_PER_VENDOR=2` to reproduce the earlier six-worker matrix.
`COMMUNICATION_VENDORS=codex,claude` restricts a run to those installed providers;
the default still includes all three vendors. A provider outage fails the selected
run rather than silently omitting its workers.

Verification runs formatting, Clippy, Rust contract tests, real CLI/MCP process tests,
and isolated skill execution. Python conformance is explicitly skipped unless its
runtime is supplied. `poc` copies the built skill to a temporary installation, checks
its instructions against the embedded skill, uses real Haiku/Luna calls, and verifies stored messages and
acknowledgements, actual tool conflict results, and lease release, and stops its owned workers. It reports a
result file in a temporary workspace. The [architecture](ARCHITECTURE.md) describes
module ownership; [review notes](docs/REVIEW.md) record defects and repairs; the [lock decision](docs/LOCKS.md)
compares the researched alternatives.
