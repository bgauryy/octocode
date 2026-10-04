**Short answer:** this is the LIFO slot. It is a one-task slot on each worker's `Core`. A task scheduled from that worker goes into the slot and is polled before the run queue. Several limits stop it from starving other tasks. All line numbers below are in `tokio/src/runtime/scheduler/multi_thread/worker.rs`.

**How it works**
- **The slot.** `Core` has `lifo_slot: Option<Notified>` (`worker.rs:122`). The doc comment says the worker checks it before the run queue, so the last scheduled task runs next. It is meant to improve locality for message-passing patterns and reduce latency (`worker.rs:117-122`).
- **Scheduling into it.** `schedule_task` only reaches `schedule_local` when the current thread is a worker of the same scheduler and still holds its core. Otherwise the task goes to the inject queue (`worker.rs:1360-1372`).
- **The push.** In `schedule_local` (`worker.rs:1392-1406`), a non-yield task with `lifo_enabled` set goes into the slot. Any task already in the slot is moved to the back of the run queue.
- **Notifying other workers.** A parked worker is notified only if the slot was already occupied, meaning a task was displaced into the queue. Filling an empty slot sends no notification.
- **Running it.** After the worker polls a task, it loops and takes `core.lifo_slot` (`worker.rs:709-730`). It polls that task directly, without going back through the normal queue and tick logic. The loop ends when the slot is empty.

**What limits it**
1. **A cap of 3 polls per tick.** `MAX_LIFO_POLLS_PER_TICK = 3` (`worker.rs:269`). The constant's comment calls the value "picked out of thin-air". Once `lifo_polls >= 3`, the worker sets `lifo_enabled = false` (`worker.rs:763-766`). Later schedules then go to the back of the run queue (`worker.rs:1392`). The code comment says this prevents ping-pong starvation, where A wakes B and B wakes A repeatedly (`worker.rs:757-762`). `lifo_enabled` is reset when the slot comes up empty (`worker.rs:730`) and in `reset_lifo_enabled` (`worker.rs:798-799`). It is also reset at `worker.rs:573` in case the core was previously stolen from a task that had the slot disabled.
2. **The coop budget.** If `coop::has_budget_remaining()` is false, the LIFO task is pushed to the back of the run queue and the worker returns instead of running it (`worker.rs:733-748`).
3. **Yields.** Yielded tasks always go to the back of the queue (`is_yield` check, `worker.rs:1392`).
4. **Stealing.** The slot cannot be stolen. When a worker shuts down, its slot task is moved to the run queue (`worker.rs:479-481`). If the core is stolen mid-loop, the loop exits (`worker.rs:714-723`).
5. **Config.** `Builder::disable_lifo_slot` turns it off (`builder.rs:131`, `builder.rs:1425-1426`). It is on by default (`worker.rs:298`, `builder.rs:368`).
6. **Scope.** Only wakes or spawns made from a worker thread that holds a core use the slot. Wakes from outside, such as the I/O driver without a core or other threads, go to the inject queue (`worker.rs:1374-1376`).

**Uncertainty:** I did not trace the I/O driver path in detail. The `park.is_some()` comment (`worker.rs:1409-1411`) suggests resource-driver wakes can reach `schedule_local` with the park taken, but I did not confirm that. I also did not check the `builder.rs` doc text. I did not confirm the commit SHA, because git failed in this sandbox. I assumed the checkout is the pinned one.