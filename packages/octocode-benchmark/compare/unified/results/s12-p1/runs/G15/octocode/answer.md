**PR #8337, "runtime: add an opt-in sharded spawn_blocking queue"**, added the opt-in. The author is alex, and it merged on 2026-08-09 as merge commit `8b13642a1f`. Its body says it "Re-lands the sharded queue from #7757 (reverted due to #8056), disabled by default." You opt in with the unstable `Builder::enable_sharded_blocking_queue` or the `TOKIO_UNSTABLE_SHARDED_BLOCKING_QUEUE` environment variable.

**The earlier attempt:**
- **#7757**, "rt: improve spawn_blocking scalability with sharded queue", merged on 2026-04-10. It was released in 1.52.0, since #8045 "prepare 1.52.0" merged 2026-04-14. Treat the release link as inferred from the PR dates.
- **#8056** reported a regression: programs using `spawn_blocking` hung.
- **#8057**, "rt: revert #7757 to fix regression in `spawn_blocking`", merged on 2026-04-16 as `56aaa43e91`. It was a plain revert, meant to ship quickly as v1.52.1. Its description says the hang still occurred with `NUM_SHARDS` set to 1, so the sharding couldn't be switched off. The sharded queue was also too tightly coupled to the rest of the `spawn_blocking` code to swap out, so an opt-in setting wasn't possible at that point.
- **#8135**, "refactor the spawn blocking queue to make adding a new sharded implementation easy", merged on 2026-07-31. It was the preparatory refactor, and #8337 then re-landed the sharding on top of it.

**Caveats:**
- I did not check that `facc6fc47e` itself contains #8337. Its merge date of 2026-08-09 makes that likely, but I did not verify it.
- An open PR, #8532 "rt: bound sharded blocking queue batches", builds on the feature. It was opened 2026-10-02 and is not part of the answer.