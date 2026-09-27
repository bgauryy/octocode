# Lock identity decision

Agents need the same answer to “does another live lease cover this path?” across
Rust, Claude, Codex, Pi, and SQLite-only integrations, including before file creation.
The lock service must not create placeholder files or depend on a model deciding
whether two paths overlap.

## Options considered

| Option | Benefit | Cost / decision |
| --- | --- | --- |
| Native filesystem comparison rules | Allows distinct case-sensitive names to proceed independently. | Volume/directory flags and Unicode versions differ; every DB client would need platform-specific logic. Defer. |
| One canonical caseless lease namespace | Same deterministic comparison in every client; handles absent names. | Case/normalization aliases conflict even when the filesystem distinguishes them. **Selected.** |
| Reserve the nearest existing parent for every new path | Avoids guessing missing filename identity. | Serializes unrelated new files in one directory. Too broad for normal agent work. |
| OS locks or a separate broker | Can enforce some process-level ownership rules. | Does not preserve the SQLite-only contract or solve arbitrary path/tree aliases by itself. Outside this repair. |

The architectural priority is exclusion with a small interoperable contract. The
product tradeoff is explicit conservative conflicts for case/normalization aliases;
ordinary unrelated paths remain independent. Avoiding another daemon or routing
model keeps the skill usable by generic agents.

## Chosen rules

1. Resolve the workspace and path against the real filesystem. Follow each symlink
   before processing a subsequent `..`; keep nonexistent suffixes as logical paths.
   Bound link traversal and propagate permission, cycle, and traversal errors.
2. Check workspace containment using the case-preserving resolved path. Folded
   comparison keys never authorize filesystem access or an escape from the workspace.
3. Compare lease components using Unicode 16 canonical caseless matching:
   `NFD(casefold(NFD(component)))`. Preserve the stored spelling. Pin the Rust data
   versions. SQL clients use the same Unicode 16 caseless comparison for path leases.
4. A file lease conflicts at an equal path. A tree lease also conflicts with
   descendants. Comparison respects component boundaries, not raw string prefixes.
5. Acquire under `BEGIN IMMEDIATE`: validate active presence, look up overlapping
   live leases, reject an overlap, or insert an acquisition ID with expiry measured
   after the writer transaction is acquired. Never keep the transaction open during
   a model call or while waiting for another agent.
6. Renew/unlock take the original acquisition ID as `leaseId` and require the live
   owner/lease. A failed renewal means stop writing. Expired leases are never revived.
   Existing same-owner overlaps still conflict; callers must retain and renew their
   existing lease.

The schema stores the folded comparison key with each lease: `pathKey` is
`"/" + NFD(casefold(NFD(component)))` for every path component (root omitted), so
equal keys alias and a tree contains exactly the keys under `pathKey + "/"`. Each
request is folded once; overlap is three indexed lookups on `(workspace,pathKey)`:
the exact key, tree leases at each ancestor key, and — for a tree request — the
range `pathKey > key+"/" AND pathKey < key+"0"` (`0` follows `/` in byte order).
Acquisition, `check_paths` and entity path filters share this query; no call loads
every active lease. Writers must supply `pathKey` (a trigger rejects a missing key);
all writers must use the current schema and comparison algorithm.

## Limits kept explicit

These are cooperative path reservations, not mandatory OS locks. They do not fence
an uncooperative writer, alias hard links, or freeze directory/symlink topology.
Use a tree reservation when changing directory structure; reacquire after a rename
or symlink change. Monotonic acquisition IDs prevent stale renew/unlock calls from
altering a newer lease, but cannot stop an arbitrary write after expiry.

The selected policy is intentionally stronger than case-sensitive filename equality.
It is a communication contract, not a claim to emulate every filesystem's Unicode,
mount, or naming semantics. Only macOS ARM64 has live vendor validation here.

## Evidence and validation

The old implementation failed regressions for symlink-plus-parent traversal and
missing-file case aliases. The repaired tests cover those paths, dangling links,
cycles, workspace escapes, canonical Unicode aliases, tree boundaries, and concurrent
case-alias contenders.

- [POSIX pathname resolution](https://pubs.opengroup.org/onlinepubs/9699919799/basedefs/V1_chap04.html#tag_04_13): link contents are resolved with the remaining path.
- [Apple APFS filename behavior](https://developer.apple.com/library/archive/documentation/FileManagement/Conceptual/APFS_Guide/FAQ/FAQ.html): case and normalization behavior vary; ASCII lowercase is insufficient.
- [Rust canonicalize](https://doc.rust-lang.org/std/fs/fn.canonicalize.html): resolves links for existing paths; missing paths require deliberate handling.
- [caseless implementation](https://docs.rs/caseless/0.2.2/src/caseless/lib.rs.html): canonical caseless comparison applies normalization and full case folding.
- [Agent Mail guard](https://github.com/Dicklesworthstone/mcp_agent_mail/blob/main/src/mcp_agent_mail/guard.py): inspected prior art uses case detection for guard matching; this is evidence of a different policy, not proof of its lease guarantees.

## Recovery and multi-path coordination

`lock_many` reserves a complete set (up to 32 paths) in one `BEGIN IMMEDIATE`
transaction. Conflicting sets cannot each receive a partial reservation. Requests
with internally overlapping aliases/trees are rejected before any insert. Every
returned ID must still be renewed/released explicitly.

A request requires `reasoning`: a nonblank explanation of why the paths are needed,
limited to 512 UTF-8 bytes. A bundle shares one reason. The reason is stored with
each lease and its audit events; renewal and release retain the original intent.

A conflict returns the conflicting lease (workspace-relative path, kind, expiry,
reason), its owner once (`owner`: ID, name, vendor, presence expiry), your held IDs,
`retryAfterMs`, and a stable keyed question carrying your reason to request a handoff.
Granted leases report workspace-relative paths; `lock_many` reports one shared expiry. Release held reservations before
waiting, ask once, and use independent work instead of a model polling loop.
Self-conflicts tell the owner to reuse its covering lease or release/reacquire.
A timeout is a cue to retry acquisition, never permission to write unlocked.

Default lease TTL is 60 seconds. Presence expires after 60 seconds without a
heartbeat, and heartbeats never renew leases. A stalled worker with a live
listener therefore loses unrenewed locks; an orderly exit releases immediately.
Each acquisition/renewal is capped at 600000 ms (10 minutes). An unrenewed lock older than 10 minutes cannot block. Deliberate renewal refreshes the deadline; presence heartbeat does not. There is
no fairness queue or automatic force-steal; atomic acquisition prevents partial
sets, while cooperation is still required for pre-existing separately held locks.
Wall-clock changes affect expiry, and advisory locks do not fence OS writes.

Research: [SQLite transactions](https://www.sqlite.org/lang_transaction.html)
document single-writer and `BEGIN IMMEDIATE` behavior. [PostgreSQL deadlock
 guidance](https://www.postgresql.org/docs/current/explicit-locking.html#LOCKING-DEADLOCKS)
recommends consistent acquisition order and avoiding long-held transactions.
[etcd leases](https://etcd.io/docs/v3.6/learning/api/#lease-api) separate TTL from
explicit keep-alive; our presence and path leases intentionally have separate
renewals. These sources inform the design, not a claim of distributed consensus.

Regression evidence: `tests/leases.test.mjs` covers concurrent reversed sets,
no partial grants, one keyed owner question, internal alias overlap, stale ID
rejection, stalled-owner expiry, orderly leave, and expired-owner cleanup.
`tests/storage-scale.test.mjs` bounds lock, conflict and `check_paths` latency with
10,000 leases and 100,000 audit rows; `check_paths` reports at most 100 conflicts
and marks a larger set with `truncated:true`.

## Before every edit and workspace discovery

A live covering file/tree lease with `reasoning` is mandatory before every edit, including creation, deletion and rename (cover both paths). Reuse an existing live covering lease; otherwise acquire one and require `ok:true`.

`locks '{}' --session <id> --workspace <repo>` lists all active locks in the bound workspace, with owner agent ID, reason, `acquiredAt`, `refreshedAt` and `expiresAt` in epoch milliseconds. MCP exposes the same `locks` tool. Follow every `{command,input}` continuation. Expired leases and leases of expired owners are excluded by default; `entity list lease '{"presence":"all"}'` is diagnostic history, never current ownership.

On conflict, send the owner one short handoff request using the returned `next`, if needed; otherwise wait or do independent work. Retry acquisition after release/expiry, and edit only after it succeeds. SQLite-only renewals must set `refreshedAt` and `expiresAt` together; the database caps expiry at refresh plus 10 minutes. The development schema changed; older databases are preserved and rejected, not migrated. Use a fresh development database.
