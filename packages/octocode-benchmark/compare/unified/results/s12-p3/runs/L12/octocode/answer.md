This is the LIFO slot optimization. All paths below are in `tokio/src/runtime/scheduler/multi_thread/worker.rs`, except the `mod.rs` doc comment.

**How it works**
- Each worker `Core` has a `lifo_slot: Option<Notified>`, separate from its `run_queue` (`:122`). Its doc comment says the last scheduled task runs next. The stated goal is locality, which helps message-passing patterns and reduces latency (`:119-121`).
- `Handle::schedule_task` (`:1360-1376`) checks that the task belongs to the current scheduler and that the current thread still holds a core. If so, it calls `schedule_local` (`:1366`). Otherwise the task goes to the inject queue (`:1372-1374`).
- `schedule_local` (`:1385`) pushes to the back of `run_queue` if `is_yield || !core.lifo_enabled` (`:1392-1395`). In every other case the new task goes into `lifo_slot` (`:1406`). Any task already in the slot is kicked to the back of `run_queue` (`:1398-1403`).
- After polling a task, the worker loops (`:713`). It takes `lifo_slot` (`:727`) and polls that task before returning to the normal queue. If the slot is empty, it calls `reset_lifo_enabled` and returns (`:729-733`).
- `next_local_task` also checks the slot first (`:1159`).

**What limits it**
1. **Poll cap:** `MAX_LIFO_POLLS_PER_TICK = 3` (`:269`). Once `lifo_polls >= 3`, `core.lifo_enabled = false` (`:763-764`). Later schedules then go to the `run_queue` until the slot is re-enabled (`reset_lifo_enabled`, `:798`). The comment explains this prevents ping-pong starvation, where task A and task B keep waking each other (`:758-762`).
2. **Coop budget:** if `!coop::has_budget_remaining()`, the slot task is pushed to the back of `run_queue` and the loop returns (`:736-749`). The runtime docs say the budget is not reset for LIFO polls (`runtime/mod.rs:380-382`).
3. **Yields:** a yield always goes to the back of the queue (`:1392`).
4. **Not stealable:** other workers cannot steal the slot task (`runtime/mod.rs:383-385`, `config.rs:40-41`). When a core is handed off, for example after `spawn_blocking`, the slot task is moved to `run_queue` so it can be stolen (`worker.rs:479-484`). If the core is stolen mid-loop, the loop exits (`:716-722`).
5. **Config:** the unstable `Builder::disable_lifo_slot` sets `lifo_enabled` to false from the start (`builder.rs:1425`, `worker.rs:298`).
6. **Same worker only:** wakes from other threads, or from a thread holding no core, go to the inject queue (`:1372`).

I read the code but did not run it. The omitted lines 771–1359 were not inspected, so I haven't checked how the LIFO task's poll is finished or timed.