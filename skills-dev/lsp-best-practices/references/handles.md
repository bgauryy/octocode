# Handles: ownership, release, cancel safety

Load when adding, changing, or debugging anything that holds a language-server resource: process, process group, connection, pending request, open document, stderr drain, or pool slot. Why: every LSP incident we've shipped was a handle that outlived its owner. Examples: a zombie reused after a failed read, grandchildren orphaned by a parent-only kill, a cancelled start that stranded its in-flight slot.

## The handle model

| Handle | Owns | Must be released on | Release action |
|---|---|---|---|
| Child process | the pid | stop, failed start, drop, pool eviction | `shutdown` → `exit` → bounded wait → group kill → **reap** |
| Process group | server + grandchildren (proc-macro-srv, cargo, tsserver workers) | every exit path, **including a clean exit** | `kill(-pgid)`. Capture the pid **before** `wait()`. |
| Memory cap | Job Object (Windows) / `RLIMIT_AS` (Linux only) | after the child is reaped | drop the guard **after** the reap |
| Connection | reader task, writer, `failed` flag | read EOF, oversized frame, write fault, (policy) timeout | set `failed`, then fail **every** pending request |
| Pending request | id → oneshot | response, timeout, **future dropped**, connection failure | remove the entry, and send `$/cancelRequest` if the request is still live |
| stderr drain | pipe reader | process exit | bounded ring **and** a bounded line read. Keep draining after decode errors. |
| Open document | `(uri, version)` | LRU eviction, explicit close, server restart | `didClose`. Re-open after a restart. |
| Lease | caller's use of a pooled client for a **whole operation** (syncs, waits, multi-request walks) | lease `Drop` | decrement. Idle **and LRU** eviction skip leased clients. Counting only in-flight requests misses the gaps between them. |
| Pool entry | one live server per key | idle TTL, LRU overflow, `clear`, `alive()==false` | stop the client. Generation-check timers. |
| In-flight start | dedupe slot for concurrent `acquire` | success, failure, **caller dropped** | a guard resolves the slot and stops a half-started child |

## Rules
- **One owner, one release path**, and that path runs on cancel, error, timeout, and drop. `kill_on_drop(true)` is only a backstop, because `Drop` can't `await`.
- **Start is one transaction.** Spawn, `initialize`, capability and encoding check, `initialized`. Any failure kills the group, kills the process, and aborts stderr. A half-initialized child never reaches the pool.
- **Supervise start in a spawned task.** Move `start()` into its own task so that a dropped caller can't strand a child halfway through startup. The caller awaits a handle whose `Drop` resolves the in-flight slot.
- **Fail everything still waiting, in one move.** Zed stores the pending map as `Mutex<Option<HashMap>>`, and both the reader and the writer `take()` it on exit. Every waiter then sees `ConnectionReset`, and new requests fail fast.
- **Tell a timeout apart from a crash.** Return `Result | Timeout | ConnectionReset` (Zed) so callers can decide whether to retry, restart, or give up.
- **Assign the id and enqueue synchronously** at call time, not on first poll, so request order is stable (Helix).
- **Deduplicate** concurrent starts and health checks per key: N callers spawn one process.
- **Idle shutdown respects busy.** Never stop a client that has active requests. Timers carry a generation so a stale timer can't remove a replacement entry.
- **Tool deadlines must exceed the readiness budget.** A caller deadline equal to the server's readiness budget cancels every cold start.
- **Restart is the caller's job.** No mature client auto-restarts at the transport layer (Zed, Helix). Mark the connection dead, restart lazily on the next acquire, with backoff and a cap, and **re-open documents**.

## Verify
- Leak loop: acquire, then stop or drop, N times. Assert no lingering PIDs, including grandchildren.
- Cancel at every `await` of start and of `request`. Assert no stranded slot, no live child, and no partial frame.
- Hung server: a request past its deadline cancels (and poisons, if that is the policy) within budget.

Next: for octocode's concrete handles and anchors load `references/octocode-engine-map.md`. For patterns from other clients, `references/rust-client-patterns.md`.
