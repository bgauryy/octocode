**Answer:** PR #8337, "runtime: add an opt-in sharded spawn_blocking queue", added the opt-in. It was merged on 2026-08-09. The earlier attempt, #7757, was merged and then reverted.

**Earlier attempt:**
- PR #7757, "rt: improve spawn_blocking scalability with sharded queue", was merged on 2026-04-10.
- It caused a regression. Issue #8056, "regression in `spawn_blocking` caused by #7757 causes tests to hang", reported it.
- PR #8057, "rt: revert #7757 to fix regression in `spawn_blocking`", reverted it. It was merged on 2026-04-16.
- PR #8045, "chore: perpare 1.52.0", was merged on 2026-04-14, between the two.
- PR #8067, "Re-land sharded spawn blocking queue", is still open and was never merged.

**Related and still open:** issue #8527, "Sharded spawn_blocking queue can leave later shards waiting under load".

**Uncertainty:**
- I did not read the diff of #8337, so I did not confirm that it gates sharding behind an unstable flag.
- I did not confirm that #8337 sits at or before facc6fc47e. Its merge date is after the revert and its number is below #8515, which that commit references, so it very likely does.
- I did not check whether #8337 builds on #7757 or on #8067's re-land.