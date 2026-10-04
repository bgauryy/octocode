**PR #8337, "runtime: add an opt-in sharded spawn_blocking queue", added the opt-in.** It was merged on 2026-08-09. I did not check that it is an ancestor of facc6fc47e. The commit itself is #8515, and its number is higher than #8337, so I assume it came after.

**The earlier attempt was merged and then reverted:**
- PR #7757, "rt: improve spawn_blocking scalability with sharded queue", was merged on 2026-04-10.
- Issue #8056, "regression in `spawn_blocking` caused by #7757 causes tests to hang", reported a regression from it.
- PR #8057, "rt: revert #7757 to fix regression in `spawn_blocking`", was merged on 2026-04-16 and backed the change out.
- PR #8067, "Re-land sharded spawn blocking queue", is still open. It seems to be a re-land attempt, and #8337 appears to have taken the opt-in route instead.

**Uncertainty:** I only read titles, states and merge dates from the search results. I did not read the PR bodies or the code. I haven't confirmed the exact flag name or the code location, or whether #8337 was written as a replacement for #8067. Issue #8527, "Sharded spawn_blocking queue can leave later shards waiting under load", is open and looks related to the opt-in.