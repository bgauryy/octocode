# Communication mesh reviews

September 24, 2026. Scope: trusted cooperating processes sharing a local SQLite
database. Two real Codex Luna workers, two Claude Haiku workers, and two Pi workers
using the configured Haiku provider. A seventh participant uses Python's SQLite
library directly, without the native runtime or any vendor executable on PATH.

## Findings and repairs

| Priority | Finding | Repair and verification |
| --- | --- | --- |
| Medium | Topic broadcast required subscribers; there was no notify-everyone operation. | Added `notify_all` to CLI, bound MCP tools, Pi's registered tools, and the direct-SQL Python example. It snapshots active peers in the same workspace, excludes the sender, and needs no subscriptions. |
| Medium | The earlier three-worker POC did not prove every directed pair or same-vendor routing. | Added `scripts/mesh-poc.mjs`: 30 worker-to-worker messages plus 30 real peer replies, including Claude↔Claude, Codex↔Codex and Pi↔Pi. |
| Medium | A model's status can contradict a successful tool result. | The first six-worker run needed one supervisor correction after Codex mislabeled a denied lock. The harness now gates lock progress on actual tool receipts and DB state, not BLOCKED/LEASED prose. |
| Low | Claiming vendor-native messaging as a universal bus would misdescribe the implementation. | Documented the actual supported bridges and the distinct meanings of native vendor messaging APIs below. |

`notify_all` reuses the existing send transaction and `(sender,key)` idempotency
rule. It stores `target="*", topic=NULL` and one delivery per active peer. Retrying
preserves the original recipient snapshot, even if peers join later. Zero recipients
is a valid stored broadcast. Existing inbox/ack clients need no schema migration.
Direct recipient IDs remain UUIDs; `*` is reserved for broadcast.

The broadcast regression failed before implementation with `Unknown command:
notify_all`, then passed. Tests cover inactive peers, another workspace, sender
exclusion, no subscribers, retry after a peer resumes, changed-content rejection,
zero recipients, and expired senders. A fault-injection trigger rejects the second
delivery: both the first delivery and its message roll back, proving atomic fanout.
Python/Rust conformance verifies broadcast in both directions.

## Actual vendor routes

| Worker | Outbound tool call | Incoming delivery |
| --- | --- | --- |
| Codex | Bound `send_message` / `notify_all` through MCP | Proxy reads committed DB deliveries, then uses app-server `turn/start` at idle |
| Claude | `mcp__agents_communication__send_message` / `notify_all` | Proxy reads DB deliveries, then writes Claude's supported streaming user input |
| Pi | Registered `send_message` / `notify_all` extension tools call the bound Rust CLI | Proxy reads DB deliveries, then submits a Pi RPC prompt after `agent_settled` |
| Generic agent | Direct SQL transactions following the v1 protocol | Read deliveries/inbox and acknowledge through SQL; own polling and heartbeat |

Every tested peer message is stored in SQLite before model delivery. There is no
model router, no vendor-to-vendor bypass, and no requirement that peer vendors match.

Claude's built-in `SendMessage` and `ListAgents` address its own session system;
[Claude documents that routing and its session inboxes](https://code.claude.com/docs/en/cross-session-messaging).
Pi distinguishes model-callable registered tools, conversation messages and external
storage in its [extension contract](https://raw.githubusercontent.com/earendil-works/pi/main/packages/coding-agent/docs/extensions.md).
Codex exposes its harness through [App Server](https://openai.com/index/unlocking-the-codex-harness/),
with MCP tool calls observable in the protocol. These are supported integration
points, not evidence that the vendors share a built-in cross-vendor `SendMessage` API.
This package's DB tools provide that common contract; the native private messaging
commands are not intercepted or renamed.

## Validation and boundaries

The deterministic suite passes 12 Rust tests and 16 CLI/process tests, with Python
conformance enabled and no skips. Formatting, Clippy and standard skill validation
pass. The release skill archive was rebuilt and extracted into a path with spaces;
`notify_all` worked with only launcher utilities on PATH, without Node, Cargo or
vendor executables. The six-worker baseline used a 337-word skill.

The first full live run passed with 166 stored messages and 208 acknowledged
deliveries. It included one explicit supervisor correction for the incorrect Codex
status described above; `out/mesh-poc-supervised.json` preserves that fact and the
original trace.

The clean rerun completed every delivery acknowledgement in **143.084 seconds**,
with **163 stored messages, 205 acknowledged deliveries, zero unexpected model
reports and zero supervisor interventions**. Each vendor process loaded the copied
skill and used its bound tools. The evidence in `out/mesh-poc.json` records 5,876
proxy events, message IDs, actual tool receipts, model selections and the skill hash.
The run token is `41273610-8dd4-4d80-afcc-ae71c120413a`.

Every cell below represents a direct message **and a real reply**, checked against
DB rows; outgoing direct messages also have matching vendor tool receipts.

| Sender → recipient | Codex 1 | Codex 2 | Claude 1 | Claude 2 | Pi 1 | Pi 2 |
| --- | --- | --- | --- | --- | --- | --- |
| Codex 1 | — | Pass | Pass | Pass | Pass | Pass |
| Codex 2 | Pass | — | Pass | Pass | Pass | Pass |
| Claude 1 | Pass | Pass | — | Pass | Pass | Pass |
| Claude 2 | Pass | Pass | Pass | — | Pass | Pass |
| Pi 1 | Pass | Pass | Pass | Pass | — | Pass |
| Pi 2 | Pass | Pass | Pass | Pass | Pass | — |

All six workers called `notify_all`: **42 deliveries**, seven recipients per call
(the other five workers, controller and DB-only client), with zero subscriptions.
The Python-only client also sent direct messages to all six workers and broadcast
to seven recipients; retrying its broadcast returned the original receipt. All six
workers encountered the held parent-tree conflict, then acquired, renewed and
released the same file lease in sequence. Cleanup verified zero remaining leases,
no active test sessions and no surviving owned vendor processes.

## Ratings

Engineering judgments for this local cooperative scope, not statistical reliability
scores or guarantees about autonomous research quality:

| Area | Rating | Evidence and remaining gap |
| --- | --- | --- |
| Direct messages | 9/10 | All 30 directed routes and replies passed, including same-vendor pairs; prolonged crash/restart soak remains. |
| Notify all | 9/10 | All six tool adapters and direct SQL passed; atomic rollback and retry snapshots have deterministic coverage. No long-duration fanout load test. |
| File/path locks | 8/10 | Six real conflicts and six acquire/renew/release cycles passed alongside path/expiry regressions. Leases remain advisory; they cannot fence arbitrary filesystem writes. |
| Coordination | 8/10 | Presence, routing, acknowledgements and orderly cleanup passed. A model misread a correct result in the first run; the controller now uses tool receipts for progress. |
| Generic DB interoperability | 9/10 | Python-only participant exchanged direct and broadcast messages with all six workers. Clients must implement the documented transactions and lifecycle correctly. |
| Overall | 8.5/10 | A working, tested local communication layer. Broader platform validation, load/crash soak and autonomous research quality remain separate work. |

Limits remain: macOS ARM64 is the validated platform; all participants share one
local database/workspace; messages can repeat after crashes; leases coordinate
cooperating writers and do not block arbitrary filesystem writes. This mesh is a
connectivity and lifecycle test, not a load soak or proof of research correctness.
No claim is made that models cannot misunderstand a correct result.

## Single-file skill and nine-worker revision

The skill now contains one Markdown file: **46 lines, 650 words**, including
frontmatter, concrete commands and the full coordination flow. It explains why to
announce intent and reserve paths, how to preserve other work, how to reserve both
sides of a rename, and why deletion needs caller/replacement verification and an
affected peer handoff. Multi-path conflicts release partial reservations before a
retry. These are cooperative agent instructions, not filesystem enforcement.

The architecture change is deliberately small: `SKILL.md` owns decisions;
`rust/catalog.json` owns command schemas; Rust Store owns transitions and SQLite.
The former skill reference moved to `docs/DB.md` and is embedded into the binary.
`db protocol` returns it with the canonical DDL offline without opening storage.
The installed skill no longer has a references directory. Pi's JavaScript adapter
only registers tools and invokes the same Rust runtime; the Python reference client
demonstrates a separate DB-only participant. Vendor lifecycle adapters remain
Codex app-server, Claude streaming input, and Pi RPC; no second implementation of
the store was introduced. No schema migration or Awareness change was needed.

This follows the [Agent Skills specification](https://agentskills.io/specification):
`SKILL.md` is the required instruction file; other folders are optional. Runtime
help supplies detailed contracts on demand. The database remains host-local because
[SQLite WAL requires processes on the same host](https://sqlite.org/wal.html).

Validation: 12 Rust and 16 native/process tests passed with no skips; formatting and
Clippy passed. The standard skill validator passed. The release archive was extracted
into a directory with spaces: it contains exactly one Markdown file, and embedded
instructions, nine tools and `db protocol` work with only launcher utilities on PATH.
The native suite also verifies that all three vendor protocols receive the exact
bundled skill once, bound to their own identity.

The repository's research-skill linter reports six errors and one recommendation:
four require research CLI/related-skill/report conventions that do not describe this
standalone runtime, two miss dynamically selected binaries/checksums, and one asks
for an additional README. Those findings are retained as explicit exceptions to
that checker, consistent with the requested single instruction file; that checker
is **not** reported as passing. The actual format and standalone execution pass.

The live harness now defaults to three workers per vendor; setting
`COMMUNICATION_WORKERS_PER_VENDOR=2` reproduces the previous matrix. The task-specific
test protocol stays in the harness rather than expanding the reusable skill.

The nine-worker run passed in **159.017 seconds**: **324 messages,
414 acknowledged deliveries, 72 directed routes with replies, nine broadcasts
and 90 broadcast delivery rows**. A Python-only participant sent direct messages to
all nine workers and a broadcast to ten recipients; its retry reused the receipt.
All nine parent-tree conflicts and acquire/renew/release cycles passed. There were
zero unexpected model reports, zero supervisor interventions, zero remaining leases,
no active test sessions and no surviving owned vendor processes.

Evidence: [out/mesh-poc-nine.json](../out/mesh-poc-nine.json), token
`23d47cc9-d1f0-4588-8ee9-04cd936c94af`, skill SHA-256
`397e6adafd649718b7c5e736c1c58022fe733e95ba9707a595d5127abee427f0`.

Each number below counts verified sender→recipient pairs with a reply. Same-vendor
cells exclude self-send; the individual 9×8 matrix and receipts are in the evidence.

| Sender → recipient vendor | Codex | Claude | Pi |
| --- | ---: | ---: | ---: |
| codex | 6 | 9 | 9 |
| claude | 9 | 6 | 9 |
| pi | 9 | 9 | 6 |

Updated engineering ratings: **skill 9/10, architecture 8.5/10, implementation
8.5/10, proxy messaging 9/10, locks/coordination 8/10**. The one-file interface and
standalone contracts are clearer; the expanded run strengthens interoperability
evidence. Ratings remain bounded by macOS-only validation, advisory locks, and the
absence of long-running crash/load tests. This test verifies communication proxies
and lock operations; it does not demonstrate autonomous repository deletion or
prove that arbitrary agents follow the new editing instructions.
