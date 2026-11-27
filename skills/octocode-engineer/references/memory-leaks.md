Load when memory grows in a long-lived garbage-collected process (Node, JVM, Go, Python).

# Memory: leaks in long-lived managed runtimes

## Leaks in long-lived managed runtimes

A garbage-collected process leaks references, not bytes. Ask first: **what repeats in one process** (a session, request, tool call, reconnect or test file), and what outlives it? One-time setup is not a leak. Something that grows with every repetition is.

| Step | How (Node shown; the same shape works for JVM, Go and Python) | Done when |
|---|---|---|
| 1. Repeat the unit | Drive N create, use and dispose cycles of the real unit in one process (not one long run) | N ≥ 10 cycles, with real work in each |
| 2. Count references | After each cycle: listeners per event on `process` and the stdio streams, `process.getActiveResourcesInfo()` by type, timers, child processes | Flat counts. Any per-cycle step is a leak with an exact owner. |
| 3. Measure the heap after GC | `--expose-gc`, `gc()` twice, then `heapUsed`, sampled at cycles 1, k and 2k | Flat, or a plateau. A cache warming up stops growing; a leak keeps its slope. |
| 4. Diff snapshots | `v8.writeHeapSnapshot()` at two cycles; aggregate self size and count by constructor or closure name; rank the delta | The growing names point at an owner (handler, registry or provider closures, not "strings") |
| 5. Walk retainers | BFS from the GC roots over strong edges (skip weak and shortcut edges), then follow parents back from samples of the grown objects | One chain names the anchor: a module-level collection, a timer, a global |
| 6. Fix at the anchor | Return and call the unsubscribe; delete on dispose; cap with LRU; make timers clearable | The probe from step 2 or 3 is flat, and a regression test fails without the fix |

Gotchas seen in practice:

- **Discarded unsubscribe.** `onSomething(cb)` on a module-level `Set` with its return value ignored keeps every disposed owner, and everything it references, alive. Search for subscription calls whose result is unused.
- **Timers capture context.** On Node 24+ each timer keeps the async-context frame it was created in (AsyncLocalStorage values). `AbortSignal.timeout(20_000)` on a 50 ms call pins that context, often a whole session, for 20 s, and it cannot be cleared. Use a clearable deadline (an `AbortController` plus `setTimeout`, cleared in `finally`).
- **Workers piped into stdio.** `new Worker()` without `stdout: true, stderr: true` pipes into the parent's streams and adds an `error` listener there per live worker. `MaxListenersExceededWarning` is the symptom; `--trace-warnings` plus a hook on `EventEmitter.prototype.on` that records stacks finds the source.
- **Registration and teardown asymmetry.** Cleanup that runs after other disposers is skipped when one of them throws. Release process-wide handlers (signals, `uncaughtException`, stream errors) in `finally`.
- **Per-key maps keyed by unique ids** (request ids, derived `<session>:<n>` ids, file paths, cwds) are never matched by a cleanup keyed on the parent id. Prefix-match the cleanup, or cap the map.
- **Keep what is cheap to keep.** Store a hash instead of the full text for dedupe. Leave a tiny monotonic counter that guards correctness (revision or generation numbers) alone: deleting it can reissue an old value.
- **Bounded but long is still a leak at churn.** A 30 s retention per call becomes the steady state when calls come every second.
- **Test runs leak too.** A runner killed before teardown leaves children behind: tmux started with `env -i`, grandchildren that ignore SIGTERM. Sweep orphans (re-parented to PID 1) whose command line names a dead run's temp folder at the next setup.
- **Tooling false alarms.** `expect(a).not.toBe(b)` in Vitest deep-compares for its hint and can trip throwing getters on closed handles. Compare identity directly.

Thinking checks: separate retention (the heap after GC grows) from churn (allocation rate) and from caches that plateau. Prove a leak with a slope over cycles, not with one large snapshot. A leak fixed in a test harness is not a product fix: say which one it was.
