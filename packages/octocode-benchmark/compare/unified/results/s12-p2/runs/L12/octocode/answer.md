This is the LIFO slot in the multi-thread worker, all in `tokio/src/runtime/scheduler/multi_thread/worker.rs` at facc6fc47e.

## How it works

- **Storage.** Each worker's `Core` has `lifo_slot: Option<Notified>`. Its doc comment says the worker checks it before the run queue, so the last scheduled task runs next. The stated benefit is locality for message-passing patterns and lower latency (`worker.rs:117-122`).
- **Push side.** `Handle::schedule_task` (`:1353`) only goes local if the calling thread is a worker of this same scheduler and still holds its core (`:1360-1369`). Otherwise the task goes to the inject queue (`:1372-1374`).
  - `schedule_local` (`:1385`) then handles the task. If it is not a yield and `lifo_enabled` is true, the task goes into `lifo_slot` (`:1396-1406`).
  - Any task already in the slot is kicked to the back of the local run queue (`:1398-1404`).
  - A parked worker is notified only when `should_notify` is true and the core has a park (`:1411-1416`). For a LIFO push, `should_notify` is true only if it displaced a previous occupant (`:1399`, `:1408`). Otherwise the slot task is expected to run on this worker anyway.
- **Run side.** After a task is polled (`task.run()`, `:704`), the worker loops (`:713`):
  1. It takes the core. If the core was stolen, it breaks out (`:716-723`).
  2. It takes the `lifo_slot` task. If the slot is empty, it resets `lifo_enabled` and returns the core to the normal loop (`:727-733`).
  3. It polls that task directly, without going through the queue (`:790`).

## Limits

1. **`MAX_LIFO_POLLS_PER_TICK = 3`** (`:265-269`).
   - Each LIFO poll increments `lifo_polls` (`:753`). On reaching 3, `core.lifo_enabled = false` (`:763-766`).
   - From then on, `schedule_local` sends tasks to the run queue (`:1392`).
   - The comment says this prevents starvation in ping-pong workloads, where A wakes B and B wakes A repeatedly (`:756-762`).
   - `lifo_enabled` is re-enabled when the slot is found empty (`:730`) and at the top of `Context::run` (`:573`).
2. **Coop budget.** If `coop::has_budget_remaining()` is false, the slot task is pushed to the back of the run queue instead of being run (`:736-750`).
3. **Yields.** `is_yield` always goes to the back of the queue (`:1392`).
4. **Config.** `disable_lifo_slot` turns it off. It sets the initial `lifo_enabled` (`:298`) and is what `reset_lifo_enabled` restores (`:798-800`).
5. **Stealing.**
   - Other workers cannot steal from the slot, only from the run queue. The `spawn_blocking` handoff code says "all tasks [except the lifo_slot] can be stolen" (`:478-484`).
   - Before a core is handed off, the slot task is moved to the run queue (`:481-484`).
   - If the core is stolen mid-loop, the loop breaks and the stealer resets the flag (`:716-723`).
6. **Locality.** Wakes from non-worker threads, or from another runtime, use the inject queue and never hit the slot (`:1363-1373`).

Only the cited ranges were read. I did not read lines 130-294 or 584-673, so I cannot say whether other code touches `lifo_slot`. A search of the multi_thread directory for "lifo" found only `worker.rs` and `counters.rs`. Only the first page of that search's results was shown.