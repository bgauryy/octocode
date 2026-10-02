This is the LIFO slot in `tokio/src/runtime/scheduler/multi_thread/worker.rs`. It is a one-task, worker-local slot that is checked before the run queue. Four limits constrain it: a cap of 3 consecutive polls, the coop budget, a per-config disable flag, and the fact that the slot cannot be stolen. I read the code at the pinned checkout and did not run it.

**How it works**

- **The slot.** `Core` has `lifo_slot: Option<Notified>`, documented at `worker.rs:117-122`. The last task scheduled from the worker is stored there. The worker checks the slot before the run queue, so the last-scheduled task runs next. The stated purpose is locality and lower latency for message-passing patterns.
- **Scheduling into it.** `Handle::schedule_task` (`worker.rs:1353-1376`) takes the local path only if the caller is on a worker thread of the same scheduler and that thread still holds its core (`:1363-1368`). Otherwise the task goes to the inject queue and a parked worker is notified (`:1373-1374`).
- **`schedule_local`.** `schedule_local` (`:1385-1417`) pushes the task to the back of `run_queue` if `is_yield || !core.lifo_enabled`. Otherwise it puts the task in `lifo_slot`. Any task previously in the slot is displaced to the back of `run_queue` (`:1398-1406`). Other workers are notified only if the slot was already occupied, and only when `core.park.is_some()` (`:1411-1416`).
- **Running it.** After a task is polled, the loop at `:709-733` takes `core.lifo_slot` and polls it. When the slot is empty, it resets `lifo_enabled` and returns. A LIFO-polled task can itself fill the slot again, so the loop continues.
- **Next-task selection.** `next_local_task` (`:1158-1160`) is `lifo_slot.take().or_else(|| run_queue.pop())`.

**What limits it**

1. **Poll cap per tick.** `MAX_LIFO_POLLS_PER_TICK = 3` (`:265-269`, "picked out of thin-air"). In the loop, `lifo_polls += 1`, and once `lifo_polls >= 3` it sets `core.lifo_enabled = false` (`:763-766`). New tasks then go to `run_queue` instead. The comment at `:756-762` says this prevents ping-pong starvation, where A wakes B and B wakes A indefinitely. `reset_lifo_enabled` (`:798-800`) re-enables it from config, called at `:730` when the slot is empty and at the top of `Context::run` (`:573`).
2. **Coop budget.** If `!coop::has_budget_remaining()`, the LIFO task is pushed to the back of the run queue and the loop returns (`:736-750`).
3. **Yields.** A task that yields (`is_yield`) never goes to the slot (`:1392`).
4. **Config.** `lifo_enabled` is initialised to `!config.disable_lifo_slot` (`:298`), so the slot can be turned off entirely.
5. **Not stealable.** The comment at `:478-480` says that, except for the slot task, all tasks can be stolen. If the core is handed to another thread, for example when `spawn_blocking` leaves no thread free, the slot task is moved to `run_queue` (`:481-484`). Also, if the core was stolen mid-loop, the loop breaks (`:714-723`).
6. **Same-worker only.** Wakes from non-worker threads, from a different runtime, or from a thread that has no core always go through the inject queue (see `schedule_task` above).

**Uncertainty**

I did not read the code that decides which wakes call `schedule_task` with `is_yield=true`. I also did not check the exact wording of the `disable_lifo_slot` builder option.