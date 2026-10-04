**Short answer:** each worker has a one-task "LIFO slot". When a task running on a worker wakes or spawns another task, the new task goes into that slot. The worker polls the slot before it looks at its run queue. Several limits stop this from starving other tasks. All paths below are under `tokio/src/runtime/scheduler/multi_thread/`.

**How it works**
- `Core` has a `lifo_slot: Option<Notified>` field (`worker.rs:122`). Its doc comment says the worker checks it *before* the run queue, so the last scheduled task runs next. The stated purpose is locality and lower latency for message-passing patterns (`worker.rs:119-121`).
- `schedule_task` checks that the caller is on a worker of this same scheduler and that the thread still holds a core. If so, it calls `schedule_local`. Otherwise the task goes to the inject queue (`worker.rs:1355-1372`).
- `schedule_local` (`worker.rs:1384-1420`) handles the local case:
  - If the slot is enabled and this isn't a yield, the new task takes the slot.
  - Any task already in the slot is pushed to the back of the run queue (`push_back_or_overflow`) (`worker.rs:1397-1406`).
  - Another worker is notified only when a previous occupant was displaced, since `should_notify` is `ret` (`worker.rs:1392-1409`).
- After a task is polled, the worker loop at `worker.rs:709-770` takes the slot's task and runs it directly. It repeats this while the slot keeps being refilled.
- `next_local_task` also prefers `lifo_slot.take()` over `run_queue.pop()` (`worker.rs:1159`).

**What limits it**
1. **Poll cap per tick.** `MAX_LIFO_POLLS_PER_TICK = 3` (`worker.rs:269`). The comment calls it a "value picked out of thin-air". Once `lifo_polls >= 3`, `core.lifo_enabled = false` (`worker.rs:756-766`). Later local schedules then go to the back of the run queue (`worker.rs:1392`). The reason given is ping-pong workloads, where A wakes B and B wakes A, which would otherwise starve other tasks. `lifo_enabled` is re-enabled by `reset_lifo_enabled` when the slot is empty (`worker.rs:727-731`, `worker.rs:798-800`). It is also reset at the top of `Context::run` (`worker.rs:571-573`), for example after a core was stolen.
2. **Coop budget.** If `!coop::has_budget_remaining()`, the slot task is pushed to the back of the run queue instead of being run (`worker.rs:737-749`).
3. **Yields.** `is_yield` always sends the task to the back of the queue (`worker.rs:1387-1392`).
4. **Not stealable.** Other workers can't steal the slot's task. When a worker gives up its core, it moves the slot's task into the run queue (`worker.rs:479-481`). `config.rs:35-42` says the slot "will" eventually become stealable and offers an unstable `disable_lifo_slot` option, which sets `lifo_enabled` to false from the start (`worker.rs:298`).
5. **Local only.** Wakes from non-worker threads, from a different runtime, or from a thread without a core go through the inject queue (`worker.rs:1371-1372`).

**Uncertainty:** I didn't read lines 770-797, where the LIFO task is actually polled, or the builder docs. The `Context::run` reset I cite (`worker.rs:571-573`) comes from grep output plus its comment. I couldn't run `git` to confirm the checkout is at the pinned commit, because `/dev/null` access is blocked.