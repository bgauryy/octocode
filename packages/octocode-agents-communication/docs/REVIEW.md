# Review and repair record

Scope: this independent package. Awareness stays outside the change. Review and
validation date: September 24, 2026.

The initial implementation made database reads pay for database setup and left
important operations undiscoverable. Its concurrency tests also missed a lease
that could expire before acquisition returned.

| Priority | Verified finding | Repair and evidence |
| --- | --- | --- |
| High | `Store.lock` calculated expiry before waiting for the writer transaction; a short lease could already be expired on success. | Calculate expiry inside the acquired transaction. A separate process holds SQLite for 1200 ms while a worker requests a 1000 ms lease; the regression failed before the repair and passes afterward. Presence and message expiry use the same ordering. |
| High | Database opening ran `CREATE TABLE IF NOT EXISTS` on existing stores, so a missing table could silently become an empty replacement. | `rust/database.rs` validates application ID, generation, and complete schema definitions. The missing-table regression checks rejection and unchanged database bytes and permissions. |
| Medium | Reads and unknown commands entered a constructor that initialized storage, changed permissions, and enabled WAL. | CLI validation precedes opening, inspection uses read-only connections, and only join/run initialize stores. Missing-file and invalid-command regressions verify absence of a new database. |
| Medium | Heartbeat, renew, unlock, and ack checked active identity separately from their writes. Another process could end the session between those statements. | Validate and mutate under one `BEGIN IMMEDIATE` transaction in `rust/store.rs`; existing stale-session and concurrent lease checks pass. |
| Medium | CLI discovery omitted lifecycle operations, join rejected vendor-session metadata, and generic resume existed only in the store API. Entity inspection and metadata updates were missing. | `rust/catalog.json`, `rust/entities.rs`, and CLI routing expose lifecycle schemas, five entity views, constrained updates, and generic resume. CLI round trips cover sender/recipient visibility, workspace isolation, subscriptions, and delivery status. |
| Medium | Direct SQL could bypass the runtime's advisory lease checks; a table list alone was insufficient as an interoperability contract. | [DB.md](DB.md) specifies the versioned transactions. The [Python client](../skills/octocode-agents-communication/scripts/sqlite_agent.py) interoperates without vendor executables, including a Python/Rust lock race. Arbitrary uncooperative SQL remains outside the contract. |

## Verification

The deterministic suite covers CLI and MCP calls, database initialization and schema
rejection, entity round trips and pagination, identity boundaries, generic resume,
read-only inspection, bounded waits and cancellation, path aliases, expiry, stale
owners, delivery reclaim after claim timeout, and concurrent writer behavior.
Python conformance requires `COMMUNICATION_PYTHON` pointing to a runtime with SQLite
3.51.3 or later; current lease conformance uses Python 3.14 (Unicode 16.0.0) with SQLite 3.53.1.

The real POC starts Haiku and Luna processes, exchanges messages tagged with a unique test token,
checks stored sender/recipient IDs and acknowledgements, and verifies lease release.
The optional three-vendor run adds Pi. Both Claude and Pi must return an actual
lock tool result naming Codex as the conflicting owner; model prose is not accepted
as evidence. Deterministic Store and cross-runtime tests also check exclusion. The POC closes the processes
it owns and leaves evidence in its reported temporary result file.

## Rust and skill migration

The runtime now lives in `rust/`. The build outputs the executable under the skill's
`scripts/bin/<target>/` folder. SQLite is bundled. Standalone execution is verified
from a copied skill with only launcher utilities on PATH. The TypeScript runtime,
its duplicate schemas, and its JavaScript client have been removed.

Port tests exposed two additional defects: generated JSON schemas marked defaulted
inputs as required, and simultaneous first-time opens could race during WAL setup.
Input schemas now mark defaults optional, Rust supplies the defaults, and database
opening retries only SQLite busy/locked errors within a bounded interval. The eight
process initialization and acquisition test passes.

Rust formatting and Clippy pass. Standard skill validation passes. The repository's
skill reviewer assumes Octocode search-tool lobby conventions, which do not describe
this standalone native skill. Its four lobby convention errors and two false unused-file reports for dynamically
selected binary/checksum assets remain documented checker mismatches. Isolated
execution verifies the real launcher-to-binary route.

## Limits retained

Leases coordinate cooperating writers; they do not fence OS writes or arbitrary SQL.
Delivery requires explicit acknowledgement and can repeat after a crash. Generic
agents provide their own polling and heartbeats. The proxy manages the vendor
processes it launches; it does not attach to arbitrary existing desktop tasks or
restore vendor conversation history. Use `--duration-ms` to bound model-backed runs.
The package remains private and unpublished.

## Proxy review

The useful frame change was to unify coordination state, not vendor protocols. A
common database and shared tool schemas allow Codex app-server, Claude streaming,
and Pi RPC to use their supported interfaces. There is no additional model acting
as a router, and no model call for an empty inbox.

- Pi was missing. Its adapter now loads a temporary native extension with the same
  tool catalog and a bound session. Each call executes the Rust binary directly.
  It waits for `agent_settled`, not an intermediate `agent_end`, and surfaces
  command/provider failures. Installed protocol reference: Pi 0.87.1.
- An idle claim acquired `BEGIN IMMEDIATE` on every poll. A read probe now skips
  empty transactions; the transaction still rechecks availability before claiming.
  A regression keeps another writer open while the idle claim returns immediately.
- The run deadline applied only after startup, and writes to vendor stdin could
  block it. Request waits and a bounded writer queue now observe the run deadline;
  cancellation/teardown stops the owned process group. Regressions use silent
  Codex/Pi startup and a Claude process that never reads its large prompt.
- The former POC accepted a model's conflict claim. Trace mode now records actual
  tool results; the POC checks the conflicting owner's ID, stored routes, explicit
  acknowledgements, and absence of a remaining lease. Trace output may include
  message content and is opt-in.
- Pi's fuzzy `haiku` selector picked an unconfigured Bedrock provider. The Pi POC
  requires `COMMUNICATION_PI_MODEL` with an exact installed provider/model. No
  authentication settings are changed or copied.

The live three-process exchange passed using Codex `gpt-6-luna`, Claude `haiku`,
and Pi's configured `guy-provider-anthropic-x/claude-haiku-4-5-20251001`. It exercised
Codex → Claude → Codex → Pi → Codex, with both peers denied the same file lease.

Earlier validation: 8 Rust contract tests and 13 native CLI/process tests passed,
with no skipped tests. Formatting, Clippy, and standard skill validation passed.
The release executable passed the three-vendor POC, including owned process
termination and expired test sessions. Local evidence is `out/proxy-poc.json`.
The rebuilt macOS ARM64 skill archive was extracted into a path containing spaces;
its help, Pi extension, and embedded skill instructions were verified.

## Adversarial lock and inbox repairs

The later independent review found two same-target lease grants and a Pi inbox
buffer mismatch. All three are repaired:

- `rust/paths.rs` resolves links before parent traversal, including missing suffixes
  and dangling links. Workspace containment remains case-preserving.
- Lease names use pinned Unicode 16 canonical caseless comparison, shared by
  acquisition and entity queries. The Python DB-only reference uses the same
  algorithm and rejects incompatible Unicode tables for leases. Case-sensitive
  filesystems intentionally share case/normalization aliases in the lease namespace.
- Native and Python pages target 256 KiB, with at most 100 rows and continuation from
  the last returned ID. Pi's 1 MiB transport now has headroom. A regression reads
  101 maximum-size, maximally JSON-escaped messages across every page and confirms
  no missing/duplicate rows or implicit acknowledgements.

The lock regressions failed before repair, then passed. The deterministic suite now
has 10 Rust and 14 native process tests, including eight concurrent case-alias
contenders and both directions of Python/Rust alias exclusion. Formatting and
Clippy pass. [Lock design](LOCKS.md) records the researched options and tradeoffs.
Stop old clients and release/expire their leases before upgrading all participants;
the SQL schema remains unchanged, but mixed comparison algorithms are unsupported.

The repaired release also passed the live three-vendor alias POC: Claude supplied
`alias/../shared.txt`, Pi supplied `REAL/SHARED.TXT`, and both actual tool results
reported Codex's lease on `real/shared.txt`. The test checks the supplied tool
arguments as well as results, then acknowledgements, lease release, child-process
termination, and session expiry. Current local evidence: `out/proxy-poc.json`.
The standalone archive was extracted and ran alias-exclusion checks with only
launcher utilities on PATH, without Node/Cargo/vendor executables.

## Skill-driven proxy verification

The proxy previously sent a separate hand-written summary instead of the actual
skill. It now includes the exact embedded `SKILL.md` once, plus bound-session
context and the user task. Process-level tests capture the initial input across
Codex app-server, Claude streaming input, and Pi RPC and compare it byte-for-byte
with the bundled skill. Node vendor stubs clear `NODE_TEST_CONTEXT` so the Node test
runner does not hijack their protocol.

The skill is 326 whitespace-delimited words (previously 472). Root help is 1,150
UTF-8 bytes (previously 8,215); it presents the workflow and command names. Each
`<command> --help` and `schema <command>` returns focused usage and inputs, including
two-word commands. Tests cover every command and ensure help never creates storage.
Lifecycle, retry, lease-alias, and vendor-model details live in the command catalog.

All 10 Rust contract tests and 16 native process tests pass, with no skips, including
Python DB-only interoperability. Formatting, Clippy, and standard skill validation
pass. The repository-specific skill reviewer still reports the six documented
convention/dynamic-asset mismatches; those were not waived as passing checks.

The two-vendor live POC passed using a copied skill installation outside the repo:
Claude Haiku and Codex Luna exchanged messages, returned an actual alias lease
conflict, acknowledged messages, and released the lease. Their owned processes
terminated and all test sessions expired. The POC records the installed skill path,
word count, and SHA-256 alongside actual tool evidence.

The release skill archive was rebuilt and extracted into a path containing spaces.
Its focused help and embedded instructions passed with only launcher utilities on
PATH. This artifact is validated on macOS ARM64 only.

The updated three-vendor skill POC also passed: Codex → Claude → Codex → Pi →
Codex. Both alias conflicts were confirmed by real tool arguments/results, with
acknowledgements and cleanup verified. Current evidence: `out/proxy-poc.json`;
the separate two-vendor run is `out/proxy-poc-codex-claude.json`.

## Collaborative research review

A subsequent three-vendor research run verified direct questions, topic fanout,
acknowledgements, retry deduplication and exclusive publication handoffs. It also
exposed model reasoning, formatting and premature-completion failures. See
[the rated review](COLLABORATION_REVIEW.md); this was not a fully autonomous pass.

## Six-worker mesh and broadcast

[The six-worker review](MESH_REVIEW.md) covers `notify_all`, all 30 directed
worker pairs, same-vendor routing, a Python DB-only peer, lock lifecycle checks,
and the correction from model status gates to actual tool receipts.
