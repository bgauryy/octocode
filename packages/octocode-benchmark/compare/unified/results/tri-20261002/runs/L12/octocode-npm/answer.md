This is the LIFO slot. When a task running on a worker wakes or spawns another task, the new task goes into a one-element slot on that worker's core. The worker runs it right after the current task finishes, ahead of its run queue. Everything below is in `tokio/src/runtime/scheduler/multi_thread/worker.rs` at facc6fc47e.

**How it works**
- Each `Core` has `lifo_slot: Option<Notified>` (`:122`) and a `lifo_enabled: bool` flag (`:126`). The doc comment says the worker checks the slot before the run queue, so the last scheduled task runs first, which helps message-passing patterns and reduces latency (`:120-123`).
- `Handle::schedule_task` (`:1353`) only goes local when three things hold. The caller is on a worker thread. The task belongs to the same scheduler (`self.ptr_eq(&cx.worker.handle)`, `:1363`). That worker still holds its core (`:1365`). Otherwise the task goes to the inject queue and a parked worker is notified (`:1373-1374`).
- `schedule_local` (`:1385`) pushes to the back of `run_queue` when `is_yield || !core.lifo_enabled` (`:1392`). Otherwise it puts the task in `lifo_slot` (`:1406`). If the slot already held a task, that earlier task is bumped to the back of `run_queue` (`:1398-1404`).
- A parked-worker notification is sent only when the slot held a previous task, since that task is now in the queue where it can be stolen (`:1408`, `:1411-1415`). So when a task fills an empty slot, no other worker is woken.
- After `task.run()` (`:704`), the loop at `:713-794` takes the slot task (`:727`) and runs it (`:790`). It repeats until the slot is empty, then it calls `reset_lifo_enabled` and returns the core (`:729-732`).

**What limits it**
1. **Poll cap.** `MAX_LIFO_POLLS_PER_TICK = 3` (`:269`, described there as a "value picked out of thin-air"). When `lifo_polls >= 3` the worker sets `core.lifo_enabled = false` (`:763-765`). Later wakes then go to the run queue until the flag is reset. This prevents starvation when two tasks keep waking each other (`:758-762`).
2. **Coop budget.** If `!coop::has_budget_remaining()`, the slot task is pushed to the back of the queue instead of being run (`:736-749`).
3. **Yields.** `is_yield` always bypasses the slot (`:1392`).
4. **Config.** `Builder::disable_lifo_slot()` (`builder.rs:1425`) sets `config.disable_lifo_slot`. `reset_lifo_enabled` derives `lifo_enabled` from it (`worker.rs:798-800`, `:298`).
5. **Stealing.** If another worker takes the core during the loop, the loop returns without running more slot tasks (`:716-723`). The stealer resets the flag at the top of `Context::run` (`:571-573`). When a core is handed off after `spawn_blocking`, the slot task is moved to the run queue so it isn't stuck (`:479-481`). That lines up with the "except the task in the lifo_slot, all tasks can be stolen" comment.
6. **Locality.** Wakes from non-worker threads, such as I/O drivers or other runtimes, never use the slot (`:1372`).

**Uncertainty:** `lifo_enabled` is only re-enabled when the slot is empty, in `reset_lifo_enabled` at `:730`, or at the top of `Context::run`. I didn't trace every other reset site, such as the `:480-481` hand-off path.