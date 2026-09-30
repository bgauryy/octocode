**Short answer:** each worker has a single-entry "LIFO slot". When a task running on a worker wakes or spawns another task, the new task goes into that slot. The worker runs it right after the current poll returns, before it looks at the run queue. Several limits stop this from starving other tasks. All line numbers below are in `tokio/src/runtime/scheduler/multi_thread/worker.rs`.

**How it works**
- **The slot.** `Core` has `lifo_slot: Option<Notified>` (`:122`). The doc comment says the worker checks it *before* the run queue, so the last scheduled task runs next. It is meant to improve locality and help message-passing patterns (`:113-121`).
- **Scheduling into it.** `Handle::schedule_task` (`:1352`) uses the local path only if the current thread is a worker of this same scheduler and still holds its core (`:1359-1367`). Otherwise the task goes to the global inject queue (`:1372-1373`).
- **The push.** `schedule_local` (`:1385`) puts the task in the slot when the wake is not a yield and `lifo_enabled` is true (`:1392-1406`). If the slot was already occupied, the previous task is pushed to the back of the local run queue, or overflows to the inject queue (`:1398-1403`).
- **Running it.** `run_task` first polls the task it was given. It then loops, taking `core.lifo_slot` and running whatever is there (`:727`). That happens ahead of `next_task` and the normal queue. If the slot is empty, it resets `lifo_enabled` and returns (`:727-733`).
- **Notifying other workers.** A wake only notifies another worker when it displaced a previous slot occupant (`should_notify = ret`, `:1398-1408`). Slot-only pushes do not wake anyone.

**Limits**
1. **Cap of 3 polls per tick.** `MAX_LIFO_POLLS_PER_TICK = 3` (`:269`). Each LIFO poll increments `lifo_polls` (`:753`). At 3 or more, `core.lifo_enabled = false` (`:763-765`), so later wakes go to the back of the queue. The comment says this prevents starvation in ping-pong workloads where A and B keep waking each other (`:757-762`). `reset_lifo_enabled` re-enables the slot once the loop ends (`:730`, `:798-800`).
2. **Coop budget.** Everything runs inside `coop::budget`. If `!coop::has_budget_remaining()`, the LIFO task is pushed to the back of the run queue instead of being run (`:738-748`).
3. **Yields.** `is_yield` wakes (`yield_now`) always go to the back of the queue (`:1392`).
4. **Global disable.** `config.disable_lifo_slot` turns it off entirely. `lifo_enabled` is initialised from it (`:297-298`) and reset from it (`:799`).
5. **Not from outside the worker.** Wakes from other threads, from a different runtime, or from a thread that lost its core go to the inject queue (`:1372`).
6. **Core loss.** If the core was stolen during the poll (for example via `block_in_place`), the loop breaks (`:715-724`). Before handing the core off, the slot task is moved to the run queue, because slot tasks can't be stolen otherwise (`:478-483`).

**Uncertainty:** I did not read the parts of the file that handle `next_task` or stealing. That the slot is not stealable is taken from the comment at `:478-480`.