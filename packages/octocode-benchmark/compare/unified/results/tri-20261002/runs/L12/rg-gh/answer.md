**Short answer:** each worker's `Core` has a one-task `lifo_slot`. When a task running on a worker wakes or spawns another task, the new task goes into that slot instead of the back of the run queue. After the current poll finishes, the worker runs the slot task before it looks at the queue. Several limits apply, listed below. All paths are in `tokio/src/runtime/scheduler/multi_thread/`.

**How it works**
- `Core::lifo_slot: Option<Notified>` is documented as being checked "before" the run queue, so the last-scheduled task runs next. The stated purpose is locality, which helps message-passing patterns and reduces latency (`worker.rs:117-122`).
- `Handle::schedule_task` takes the local path only if the task belongs to the current scheduler and this thread still holds a core (`worker.rs:1353-1368`). Otherwise the task goes to the inject queue and a parked worker is notified (`worker.rs:1371-1373`).
- `schedule_local` puts the task in the slot when it is not a yield and `lifo_enabled` is true (`worker.rs:1392-1406`). Any task already in the slot is pushed to the back of the run queue, where it can be stolen (`worker.rs:1398-1403`). The slot itself cannot be stolen from.
- After `task.run()`, the worker loops. It takes the slot task and runs it directly, skipping the queue (`worker.rs:709-733`, `worker.rs:~790`). If the slot is empty, it resets `lifo_enabled` and returns the core (`worker.rs:727-733`).
- Notification is only sent when the slot already held a task (`ret = prev.is_some()`, `worker.rs:1399`). A task placed in an empty slot does not wake another worker, because this worker will run it right away. It is also only sent if `core.park.is_some()` (`worker.rs:1412`).

**What limits it**
1. **Poll cap per tick.** `MAX_LIFO_POLLS_PER_TICK = 3` (`worker.rs:265-269`). Once `lifo_polls >= 3`, `core.lifo_enabled = false` (`worker.rs:756-766`). Later wakeups then go to the back of the run queue. The comment says this prevents ping-pong starvation, where A wakes B and B wakes A indefinitely. `reset_lifo_enabled` re-enables the slot when the loop ends with an empty slot (`worker.rs:730`, `worker.rs:798-800`).
2. **Coop budget.** If `coop::has_budget_remaining()` is false, the slot task is pushed to the back of the run queue instead of running (`worker.rs:736-750`).
3. **Yields.** `is_yield` always pushes to the back of the queue (`worker.rs:1392`).
4. **Same worker only.** The waker must run on a worker thread of this scheduler that still holds its core. Wakes from other threads, from another runtime, or from a worker that has lost its core go to the inject queue (`worker.rs:1359-1373`).
5. **Core stolen mid-loop.** If the core is gone, the loop breaks (`worker.rs:713-724`).
6. **Only one slot.** A second LIFO schedule displaces the first into the run queue (`worker.rs:1398-1403`).
7. **Shutdown.** When a worker shuts down its core, it moves the slot task into the run queue so it can be stolen (`worker.rs:479-482`).
8. **Config.** `Builder::disable_lifo_slot()` sets `disable_lifo_slot`, and `lifo_enabled` is initialised to its negation (`builder.rs:1425-1426`, `worker.rs:298`). It is also a multi-thread-only option, and I did not check `builder.rs:1417` for the `tokio_unstable` gating.

**Uncertainty:** The line `worker.rs:~790` for the end of the loop is approximate. I saw the loop body but did not number its closing lines. I did not trace the `Waker` → `schedule_task` call path, or the `ptr_eq` check, beyond what `schedule_task` shows.