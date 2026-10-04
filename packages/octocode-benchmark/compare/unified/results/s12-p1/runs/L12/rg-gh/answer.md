**Short answer:** each worker's `Core` has a one-task LIFO slot. When a task running on a worker wakes or spawns another task into the same scheduler, that task goes into the slot instead of the back of the run queue. The worker polls the slot right after the current task returns, before it looks at the queue. Several limits apply. They are a cap of 3 slot polls per tick, the coop budget, `yield_now` and non-local wakes bypassing the slot, and the `disable_lifo_slot` config. Tasks in the slot can also be stolen, but only after being moved to the run queue. All line numbers below are in `tokio/src/runtime/scheduler/multi_thread/worker.rs`.

**How it works**
- **The slot.** `lifo_slot: Option<Notified>` is documented at `worker.rs:117-122`. The worker checks it before the run queue, so the last-scheduled task runs next. The stated benefits are locality and lower latency for message-passing patterns.
- **Scheduling into the slot.** `schedule_task` calls `schedule_local` (`worker.rs:1360-1368`) only if the task belongs to the current scheduler and the current thread still holds a core.
  - In `schedule_local` (`worker.rs:1392-1406`), when it is not a yield and `lifo_enabled` is true, the new task takes the slot.
  - Any previous occupant is pushed to the back of the run queue (`push_back_or_overflow`).
  - `should_notify` is true only if the slot was already occupied, so other workers get woken only in that case. The `core.park.is_some()` check applies on top of that.
- **Running it.** After `task.run()`, the loop at `worker.rs:709-790` takes the slot, polls it, and repeats while new tasks keep landing in the slot.

**What limits it**
1. **Cap on consecutive polls.**
   - `MAX_LIFO_POLLS_PER_TICK = 3` (`worker.rs:265-269`) is described as "picked out of thin-air".
   - Once `lifo_polls >= 3`, `core.lifo_enabled = false` (`worker.rs:756-766`). Later wakes then go to the run queue.
   - The comment says this prevents starvation in ping-pong workloads, where A wakes B and B wakes A.
   - `reset_lifo_enabled` (`worker.rs:798`) re-enables the slot when the slot is found empty and the loop exits (`worker.rs:730`), and when a worker takes over a core (`worker.rs:573`).
2. **Coop budget.** If `!coop::has_budget_remaining()` (`worker.rs:736-749`), the slot task is pushed to the back of the run queue and the loop returns.
3. **Yields skip the slot.** `is_yield` forces `push_back_or_overflow` (`worker.rs:1392`).
4. **Wakes from outside the worker.** If the waker runs on a thread with no core, or on another runtime, the task goes to the inject queue via `push_remote_task` (`worker.rs:1370-1374`). The slot is not used.
5. **Config.** `Builder::disable_lifo_slot` (`builder.rs:1425`) sets `disable_lifo_slot`. It is passed into `lifo_enabled` at `worker.rs:298`.
6. **Core stolen mid-run.** If another worker has taken the core, the loop returns `Break` (`worker.rs:713-724`).
7. **Stealing.** The slot task can't be stolen directly. When a core is being run elsewhere or shut down, the slot task is moved into the run queue first (`worker.rs:479-481`).

**Uncertainty:** I didn't trace the `Context::run` and `park` paths in detail. The "notify only if the slot was already occupied" behaviour is read from `schedule_local` alone.