**Answer:** This is the LIFO slot. Each worker's `Core` has a one-task `lifo_slot` (`worker.rs:122`). When a task running on a worker wakes or spawns another task, that task goes into the slot instead of the back of the run queue. After the current poll ends, the worker runs the slot task before it looks at the queue. This keeps the wakee on the same thread, which helps cache locality and avoids a cross-thread handoff. All paths below are in `tokio/src/runtime/scheduler/multi_thread/worker.rs`.

**How it works**
- **Scheduling** (`schedule_task` → `schedule_local`, ~1360–1410):
  - The task is only scheduled locally if it belongs to the current scheduler (`ptr_eq`) and the current thread still holds a core (~1360–1367). Otherwise it goes to the inject queue (`push_remote_task`) and a parked worker is notified.
  - If the call is a yield (`is_yield`) or `!core.lifo_enabled`, the task goes to the back of the run queue (1392–1395).
  - Otherwise the task takes the slot (1398–1406). A previous occupant is pushed to the back of the run queue, which makes it stealable.
  - A peer worker is notified only when `should_notify` is true and the core is not in a driver park (`core.park.is_some()`). In the LIFO branch, `should_notify` is true only if the slot was already occupied, since that displaced task is now stealable. A plain fill of an empty slot does not wake anyone.
- **Running** (~709–790): after `task.run()`, a loop takes `core.lifo_slot`.
  - If the slot is empty, it calls `reset_lifo_enabled` and returns the core (~727–733).
  - If the slot has a task, it runs it and loops again. That task may fill the slot again.
  - Each pass re-takes the core from `self.core`. If another worker stole the core, the loop breaks (~714–723).

**What limits it**
1. **Poll cap per tick:** `MAX_LIFO_POLLS_PER_TICK = 3` (269). Once `lifo_polls >= 3`, the code sets `core.lifo_enabled = false` (763–766). Later schedules then go to the run queue. The comment says this prevents starvation in ping-pong workloads where A wakes B and B wakes A (~757–762). `lifo_enabled` is reset to the config default when the slot is found empty (798–800) and at the top of `Context::run` (573).
2. **Coop budget:** if `!coop::has_budget_remaining()`, the slot task is pushed to the back of the run queue (`push_back_or_overflow`) instead of being run (~736–747).
3. **Yields:** `is_yield` always bypasses the slot (1392).
4. **Stealing:** the slot is not stealable. When a worker parks or gives up its core, it moves the slot task to the run queue (~479–481). The only exposure to other workers is the displaced-task push in point 5 and that hand-off.
5. **Displacement:** the slot holds one task. A second local schedule pushes the earlier one to the run queue (1398–1403).
6. **Config:** `Builder::disable_lifo_slot` (`builder.rs:131`, `:1425`) turns it off. It feeds `lifo_enabled: !config.disable_lifo_slot` (297–298).
7. **Not on this worker or without a core:** wakes from other threads, or from a thread that has no core, go to the inject queue.

**Uncertainty:** I did not read the `Context::run` code around 479 and 573 in full. The hand-off on park and the reset at the top of `run` come from grep hits plus their comments, not from the surrounding logic.