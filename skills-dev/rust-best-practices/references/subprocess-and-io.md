# Subprocess & I/O — spawning children without leaks or OOM

Load when spawning child processes (language servers, compilers, CLIs) or reading their output. Why: a spawned process outlives a careless parent — zombies, orphans, and unbounded pipes are the classic production failures (a leaked language-server pool that OOMs the host is a real, recurring bug).

## Kill AND reap — every exit path
- A killed child is not gone until it's **reaped** (`wait`/`try_wait`). Skipping the wait leaves a zombie (Unix) holding a PID slot.
- Track the child (`Option<Child>`), and on stop/error/timeout: attempt graceful shutdown, then `kill()` **and** `wait()`.
- **Graceful-then-forceful**: for protocol servers (LSP), send the protocol shutdown (`shutdown`+`exit`), wait a bounded time (~1–2s) for self-exit, then force-kill. Wire this into *automatic* teardown, not just an explicit call — the leak happens on the path you forgot.
- **`Drop` can't `await`.** In an async `Drop` you can only `start_kill()` + `try_wait()`; a still-running child then depends on the runtime's orphan reaper. Set `Command::kill_on_drop(true)` as the backstop, but don't rely on it as the primary — route real teardown through an explicit async `stop()`.
- **Group-kill** a process and its grandchildren: put it in its own process group (Unix `setsid`/`pre_exec`) or a Windows **Job Object**, and kill the group. `kill()` on the parent alone leaks grandchildren.

## Bound every resource
- **OS memory cap on the child** (not just your buffers): `setrlimit(RLIMIT_AS)` via `pre_exec` (Unix) or a Job Object memory limit (Windows). Without it a runaway child OOMs the *host* — your internal caps don't help.
- **Cap every read from a child**: max frame/message size, max header size, bounded stdout accumulation, a ring buffer for stderr (last N lines). Check a declared length (e.g. `Content-Length`) **before** allocating a buffer for it.
- **Pool bounds**: max live servers, LRU + idle-timeout eviction, dedup concurrent starts so N callers spawn one process.
- **Timeouts** on every request/write; on timeout cancel the request and mark the connection dead so the pool evicts it — don't wedge.

## Async hygiene
- Never call blocking `std::fs`/`std::process` (sync `open`/`read`/`wait`) on an async executor thread — use `tokio::fs`/`tokio::process` or `spawn_blocking`. One sync `File::open` in an async `start()` is an easy-to-miss stall.
- Sandbox the child env: `Command::env_clear()` + an explicit allowlist; pipe stdio you control.

## Verify
- Leak test: spawn→drop N children in a loop, assert no lingering PIDs. Timeout test: a hung child is killed+reaped within the budget. See `references/safety-and-security.md` for the untrusted-output angle.

Next: for the async-pool/threading side, load `references/performance.md`; for input bounds and unsafe `pre_exec`, `references/safety-and-security.md`.
