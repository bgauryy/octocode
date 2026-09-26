# Communication skill and CLI manifest

Committee decision: keep one session identity, one SQLite coordination store, one command catalog and one delivery owner per identity. Connect the existing features through a short workflow. Improve ambiguous contracts before adding another registry, task engine or memory service.

This document separates verified behavior from proposed changes. The executable catalog (`<command> --help`) owns exact fields; [SKILL.md](../SKILL.md) owns the complete command map and agent instructions.

## Connected workflow

```text
session (identity + task + declared status)
    → peers / changed-directory context → exact recipient ID
    → send_message → delivery receipt → required work → reply + ACK
    → lock / lock_many → guarded structured edit when configured → unlock
    → share_document + context metadata → scoped context → read_document
```

The same session ID owns messages, reservations, publications and audit attribution. A delivery receipt means submitted to the host; an ACK means handled by the recipient. Neither is permission to edit. Host-native IDs are bindings, never substitute database recipient IDs. Generic host collaboration tools are a separate transport unless explicitly bridged; this committee used this skill's CLI and shared database.

## Capability contract

| Capability | Implemented behavior | Boundary |
| --- | --- | --- |
| Agent entity | `session`: exact ID, name, vendor, optional host session, task, status, expiry | No second agent/profile/contact registry |
| Ongoing work | Declared `busy`; also `available`, `blocked`, `unknown` | Status is coordination data; it neither renews presence nor grants a lease |
| Presence | Join/reuse → heartbeat/listen → leave; expired identity may resume under the existing rules | 60-second presence; renew every 15 seconds when unmanaged; resume does not resurrect old leases |
| Discovery | `peers` derives active sessions; host context supplies bounded changed views | Public directory includes the caller; exclude your own ID when choosing a collaborator. Directory pages are live, not a frozen roster |
| Messaging | Durable direct/topic deliveries; retry key scoped to sender; reply correlation; atomic final reply + ACK | Same key must retain identical fields. No exactly-once external side-effect claim |
| File reservation | Advisory file/tree leases; atomic all-or-none sets; owner-specific lease IDs | Both presence and lease must remain live; heartbeat never renews leases |
| Guarded edits | Optional Pi, Claude and OpenCode adapters reject supported unleased structured edits before execution | Point-in-time admission only; no arbitrary shell/editor/custom-tool fence or protection through mid-write expiry |
| Shared memory | Immutable document bodies plus scoped summary/path/branch/expiry metadata | Context is metadata lookup, not semantic search. Default discovery TTL is one day, maximum seven; expired notes remain evidence readable by name |
| History and traces | Entity views and append-only audit associate identity, message, delivery and dispatch; worker traces correlate host calls | Audit is evidence, not another queue; host usage counters may overlap |
| Maintenance | Diagnostics, verified DB export and history-preserving compaction; prune expired leases | Document files need separate backup; no age-based deletion of protocol history |

Update your own task/status through `heartbeat` or `entity set session <id>`. Today, `entity list session` uses `status:active|expired|all` to filter **presence**, while the returned entity's `status` describes availability. This naming collision is real; the proposed rename below is not implemented.

## Lock guarantee and stronger-lock decision

A cooperative writer acquires every affected path, proceeds only on `ok:true`, preserves peer edits, renews each lease while working, and stops if renewal fails. A rename reserves both endpoints; `lock_many` grants all requested paths or none. On conflict, inspect the owner and returned handoff guidance, release held leases, and coordinate before retrying. Do not hold a database transaction across editing or model/network work.

Path resolution covers supported symlink traversal and conservative case/Unicode aliases. It cannot freeze filesystem topology or identify every hard-link alias. `check_write` validates current owned coverage in a read snapshot; a later expiry, release or path change can invalidate that result before the edit occurs.

**Stronger protection requires a separate, controlled mutation path.** Prototype a trusted writer only if preventing uncooperative writes is a requirement. It must validate ownership at mutation time, reject stale owners, protect path resolution, and prevent alternate mutation paths through host policy. A lease row, preflight hook or portable OS-lock wrapper alone is insufficient. Do not label the current mechanism an OS write fence. See [locks](LOCKS.md) and [host admission](HOST_LEASE_GUARDS.md).

## Pagination and bounded responses

Follow continuations to terminal before asserting absence. Preserve filters and cursor types; never increment or fabricate a cursor. For live directories, refresh after expiry or a failed contact rather than assuming a globally stable snapshot.

| Surface | Current continuation / bound |
| --- | --- |
| `peers`, `inbox`, `entity list` | Scalar `next` becomes `after` with the original filters; up to 100 rows and a 256 KiB target; an oversized first row is returned to ensure progress |
| `context` | `next` is the complete input for the same command, including a fixed `through` ceiling; scans at most 200 document records per call; an empty page can still have `next` |
| `read_document` | `next` is the same-command input containing name, byte offset and limit; UTF-8 boundaries and content hash are checked |
| `activity` | `next` is same-command input, with a snapshot and normalized time filter; scan caps expose truncation/limit diagnostics; nested path summaries disclose omission |
| `health`, `db retention` | `next` contains `command` and `input`; bounded diagnostic pages |
| Host peer context | Small changed view; executable directory `next`/`refresh`; removal from the view does not prove departure |
| `check_paths` and bounded conflict decisions | Explicit `truncated` diagnostic; not a full enumeration. Use the indicated lease/entity inspection path for additional evidence |

The committee found heterogeneous continuation syntax, not a proven dropped-page bug. A fresh CLI probe created 105 agents and 105 messages: peers, session entities, inbox and sent-message entities each traversed `[100, 5]` with 105 unique IDs. An 18-agent fixture would not cross the public page boundary.

## Storage and efficiency

Use one local SQLite database shared by participants on the same host and the same canonical workspace. WAL allows concurrent readers with serialized writes; this is not a multi-machine shared-filesystem design. The runtime enables foreign keys, a bounded busy timeout and full synchronous durability, with short writer transactions. [SQLite isolation](https://sqlite.org/isolation.html) and [WAL constraints](https://sqlite.org/wal.html) explain those boundaries.

Inbox, path, owner, session, dispatch, reply/conversation and document-registry indexes support their corresponding lookups. Context scans a bounded publication registry rather than loading bodies. Message bodies are not duplicated into the audit. Changed peer views suppress repeated directory injection; large evidence is published once and referenced by name. These mechanisms bound individual work; retained history still grows and needs disk-capacity monitoring. Do not claim unlimited throughput or bounded database size.

## Decisions and next implementation slices

| Decision | State | Acceptance |
| --- | --- | --- |
| Correct blanket “all mutations need reasoning” instructions | Applied during committee | ACK works with its actual schema; instructions no longer encourage an invalid field |
| Explain ongoing status, guarded edits and continuation use in the skill | Applied during committee | An agent can update its own status and distinguish reservation from guarded admission |
| Preserve one session entity and derived peers view | Accepted architecture | No duplicate agent/profile/routing state |
| Rename session/lease list presence filter to `presence` | Proposed next contract cleanup | Change catalog, runtime, callers, docs and tests together; `status` retains availability meaning; no legacy alias |
| Standardize collection continuation as `next:{command,input}` | Proposed next contract cleanup | Every next call preserves complete filters and cursor types; cross a real row/byte boundary; update all consumers together; do not promise snapshot isolation where none exists |
| Add enforced mutation service | Prototype only if stronger isolation is required | Expiry/reacquisition during a paused write rejects the stale owner; alternate mutation paths are demonstrably blocked |
| Add persistent agent profiles, status synonyms, task engine or vector memory | Rejected for current scope | Reconsider only with a concrete requirement the existing session/context model cannot satisfy |

Do not migrate old development formats or maintain parallel wire shapes. Proposed changes above are not claims about current CLI behavior.

## Committee method and acceptance evidence

Three Terra reviewers covered locks/recovery, database/context/pagination, and product/identity/status with primary web research. They discovered peers, asked each other questions, replied and ACKed through the shared skill database, and published immutable findings with scoped memory metadata. The chair verified decisive claims and corrected an initial reviewer error that confused the internal peer snapshot with public `peers` pagination.

The review retained a useful disagreement: a uniform continuation wrapper simplifies agent use, but current scalar cursors already work. Treat it as an atomic contract cleanup, not an emergency correctness fix. Similarly, stronger write fencing is a separate product capability, not a wording change to `lock`.

Validation for this review: 17 host-admission tests passed; live task/status transitions passed; four 105-row CLI pagination chains exhausted without duplicate IDs. Per-run evidence and committee receipts are under `.octocode/communication-committee/`; these are development evidence, not shipped runtime files. Existing tests additionally cover atomic competing lease sets, stale IDs, context scan gaps/expiry and immutable document reads.

## Sources

- [Command catalog](../src/catalog.json), [entity views](../src/entities.rs), [peer views](../src/peers.rs): identity, status and command contracts.
- [Store](../src/store.rs), [documents](../src/documents.rs), [activity](../src/activity.rs), [health](../src/health.rs), [retention](../src/retention.rs): continuation implementations.
- [Leases](../src/leases.rs), [write admission](../src/lease_guard.rs), [paths](../src/paths.rs): lock boundaries.
- [Database](../src/database.rs), [schema](../src/schema.sql): durability, indexes and single-store authority.
- [SQLite isolation](https://sqlite.org/isolation.html), [SQLite WAL](https://sqlite.org/wal.html): local concurrency and filesystem limitations.
- [Linux flock manual](https://man7.org/linux/man-pages/man2/flock.2.html): ordinary Linux file locks are advisory; this is not evidence of a portable mandatory fence.
