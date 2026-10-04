**Short answer:** each worker's `Core` has a one-task LIFO slot. When a task running on a worker wakes or spawns another task, that task goes into the slot instead of the back of the run queue. The worker polls the slot before anything else once the current task finishes. Several limits cap this: a 3-poll cap, yields, coop budget, stealing, and the scheduler-handle and core checks. All lines below are in `tokio/src/runtime/scheduler/multi_thread/worker.rs`.

**How it works**
- **Storage:** `Core` has `lifo_slot: Option<Notified>` and `lifo_enabled: bool` (`:122`, `:126`). The doc comment at `:118-121` says the slot is checked before the run queue, so the last scheduled task runs next. It is described as a locality optimization for message-passing patterns that also reduces latency.
- **Scheduling (`Handle::schedule_task`, `:1353`):** the task is scheduled locally only if the calling thread is a worker of this same scheduler and still holds its core (`:1363-1366`). Otherwise it goes to the inject queue and a parked worker is notified (`:1373-1374`).
- **Slot insertion (`schedule_local`, `:1385`):** when the slot is enabled and the schedule is not a yield, the new task takes the slot. Any previous occupant is pushed to the back of the run queue (`:1398-1406`). That displacement is the only case where a notify is issued on this path (`:1392-1408`), so another worker can steal the displaced task.
- **Consumption:**
  - `next_local_task` takes `lifo_slot` first, then pops the run queue (`:1158-1159`).
  - After the current task's `task.run()`, the loop at `:713-794` keeps taking the slot and running its task. It returns `Continue(core)` once the slot is empty (`:727-733`). It also calls `reset_lifo_enabled` at that point.

**What limits it**
1. **Poll cap of 3 per tick.** `MAX_LIFO_POLLS_PER_TICK = 3` (`:269`). The comment at `:265-268` says the value was "picked out of thin-air". The loop counts `lifo_polls` and, once it reaches 3, sets `core.lifo_enabled = false` (`:763-765`). From then on, locally scheduled tasks go to the back of the run queue (`:1392`). The comment at `:758-762` says this prevents starvation in ping-pong workloads, where task A and task B keep waking each other. The flag is re-enabled via `reset_lifo_enabled` (`:798-799`), which `Context::run` also calls (`:573`), once the slot is empty.
2. **Yields never use the slot.** `is_yield` forces `push_back_or_overflow` (`:1392-1395`).
3. **Coop budget.** The LIFO loop runs inside `coop::budget`. If `!coop::has_budget_remaining()`, the slot task is pushed to the run queue back instead of being run (`:736-749`).
4. **Config switch.** `disable_lifo_slot` initializes `lifo_enabled` to false (`:298`, `:799`).
5. **Stealing and core handoff.**
   - The slot is not stealable, because only the run queue is. If the worker is about to hand off its core, for example after `spawn_blocking` use, the slot task is moved to the run queue (`:478-484`).
   - If the core was stolen during the poll, the loop exits with `Break` (`:716-723`).
6. **Scope.** Wakes from outside the worker, or from another runtime, go to the inject queue and bypass the slot (`:1372-1374`).

**Uncertainty:** I did not read lines 129-689 or 801-1059 in full, so I have not checked the surrounding `Context::run` logic. That includes the call around `:481` and how `tick` and the global-queue interval interact with the slot. I did not trace the `with_current` internals either.