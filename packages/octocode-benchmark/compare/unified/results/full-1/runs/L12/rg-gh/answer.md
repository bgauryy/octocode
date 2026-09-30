**Short answer:** Each worker has a one-task "LIFO slot". A task scheduled from that worker's own thread goes into the slot instead of the run queue. After the current task's poll finishes, the worker runs the slot's task before anything else. This keeps the wakee on the same thread, which helps cache locality and message-passing latency. Several limits cap it. All line numbers below are in `tokio/src/runtime/scheduler/multi_thread/worker.rs`.

**How it works**
- The field is `lifo_slot: Option<Notified>` (`:122`). Its doc comment (`:116-121`) says the worker checks it before the run queue, so the last-scheduled task runs next.
- `schedule_local` (`:1381-1415`) handles scheduling from a worker thread:
  - If the wake is not a yield and `lifo_enabled` is true, the new task goes into `lifo_slot` (`:1397-1406`).
  - Any task already in the slot is kicked to the back of the local run queue (`:1398-1404`).
  - A peer worker is notified only if a task was displaced (`:1399`, `:1415`). Otherwise the wakee is expected to run right after the current task.
- In `run_task` (`:709-796`), after `task.run()` the worker loops:
  - It takes `core.lifo_slot` and polls it directly (`:727`, `:796`).
  - It does this without going back through the normal queue or global-queue checks.
  - When the slot is empty it calls `reset_lifo_enabled` and returns the core (`:728-733`).
- Because the slot's task is not in the run queue, it can't be stolen. When a worker parks or shuts down, the slot task is moved to the run queue so it stays reachable (`:479-481`).

**What limits it**
1. **Poll cap per tick:** `MAX_LIFO_POLLS_PER_TICK = 3` (`:269`). It is described as "picked out of thin-air", and more than 3 is judged over-weighting (`:265-268`).
   - Each slot poll increments `lifo_polls` (`:753`).
   - At 3, `core.lifo_enabled = false` (`:763-765`).
   - From then on, `schedule_local` sends tasks to the run queue (`:1392`) until the flag is reset.
   - The comment (`:757-762`) explains this prevents ping-pong starvation, where task A and task B keep waking each other.
2. **Coop budget:** if `!coop::has_budget_remaining()`, the slot task is pushed to the back of the run queue instead of being run (`:737-750`).
3. **Yields:** `is_yield` always goes to the back of the queue (`:1392`).
4. **Core stolen:** if another worker took the core while the task ran, the loop exits (`:712-723`). The stealer resets the flag (`:571-573`).
5. **Config:** `Builder::disable_lifo_slot` sets `disable_lifo_slot` (`builder.rs:131`, `:1425-1426`). It is documented as an unstable option. `lifo_enabled` starts as `!config.disable_lifo_slot` (`:298`).
6. **Scheduling context:** only tasks scheduled from a worker with a core use the slot. Otherwise the task goes to the inject queue (`:1373-1375`).

**Uncertainty:** I didn't check the tests or the git history for the reasoning behind the cap of 3. The `builder.rs` line numbers come from a grep, and I didn't read the surrounding docs.