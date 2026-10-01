**Short answer:** each worker has a single-task "LIFO slot". When a task running on a worker wakes or spawns another task, the new task goes into that slot instead of the back of the run queue. The worker polls it right after the current task finishes. Three things limit this: a per-tick cap, the coop budget, and exclusions for yields and non-worker wakes. All of it is in `tokio/src/runtime/scheduler/multi_thread/worker.rs`.

**How it works**
- **Push side, `Handle::schedule_local` (~1385).** `schedule_task` only calls it when the caller is on a worker of this same scheduler and that worker still holds its core (~1350-1358).
  - If the task is not a yield and `core.lifo_enabled` is true, it goes into `core.lifo_slot`.
  - Any task already in the slot is bumped to the back of the run queue with `push_back_or_overflow`.
  - Notifying a parked worker is skipped unless the slot was already occupied (`should_notify = ret`, where `ret` is whether the slot was occupied). A worker that keeps the task for itself doesn't need to wake another.
- **Run side, after `task.run()` (~704-767).** A loop takes `core.lifo_slot` (~727) and polls it, repeating while the slot keeps refilling.
  - If the slot is empty, it calls `reset_lifo_enabled` and returns the core to the normal loop (~730-732).
  - `reset_lifo_enabled` (~798) sets `lifo_enabled = !config.disable_lifo_slot`.
- **Wake-from-running-task pattern.** Nothing tracks "woken by the running task" explicitly. The slot is filled only by the scheduling path above, which is used by a wake or spawn made from a worker thread.

**Limits**
1. **Poll cap per tick.** `MAX_LIFO_POLLS_PER_TICK = 3` (~269). The loop counts `lifo_polls` (~753), and at 3 it sets `core.lifo_enabled = false` (~763-764). Later schedules then go to the run queue until the slot is reset. The code comment says this stops ping-pong starvation, where task A wakes B and B wakes A. The constant's comment calls the value "picked out of thin-air", meaning it is not tuned.
2. **Coop budget.** If `!coop::has_budget_remaining()` (~736), the LIFO task is not run. It is pushed to the back of the run queue and the loop returns (~738-749).
3. **Yields.** `is_yield` always goes to the back of the queue (`schedule_local`, ~1391).
4. **Config.** `config.disable_lifo_slot` turns the feature off entirely (~799).
5. **Lost core.** If another worker took the core while the task ran, the loop breaks (~716-724). The stealer resets the flag at the top of `Context::run`.
6. **Non-worker wakes.** Wakes from outside a worker, from a different runtime, or from a worker without a core go to the inject queue (`push_remote_task`), not the slot.

**Uncertainty:** the line numbers for `schedule_local` and the early part of `schedule_task` are approximate, because I read them from a window and counted. The `worker.rs:481` hit (`core.lifo_slot.take()` in another path) and whether other workers can steal from the slot were not examined.