**Answer:** PR **#8337**, "runtime: add an opt-in sharded spawn_blocking queue" (by alex), added the opt-in. It was merged on 2026-08-09 as merge commit `8b13642a1f`. I did not check whether that commit is an ancestor of `facc6fc47e`. I'm assuming it is, since `facc6fc47e` was named as having the feature.

- **How to enable it:** the PR body says it is disabled by default. You opt in with the unstable `Builder::enable_sharded_blocking_queue` or the `TOKIO_UNSTABLE_SHARDED_BLOCKING_QUEUE` environment variable.
- **Relation to the earlier attempt:** the body says it "Re-lands the sharded queue from #7757 (reverted due to #8056)". It also lists "Refs: #8067".

**The earlier attempt:**
1. **#7757** ("rt: improve spawn_blocking scalability with sharded queue", by alex) was merged on 2026-04-10. It was released in 1.52.0 (#8045, merged 2026-04-14), per the title of that PR. I did not open its contents to confirm it included #7757.
2. It caused a regression: programs using `spawn_blocking` could hang (issue #8056).
3. **#8057** ("rt: revert #7757 to fix regression in `spawn_blocking`", by hawkw) reverted it on 2026-04-16. It was merged as `56aaa43e91`, and the body says it aimed to get a v1.52.1 fix out quickly.
4. #8057's body says the hang still occurred with `NUM_SHARDS` set to 1. The sharded queue was too tightly coupled to the rest of the `spawn_blocking` machinery to toggle on and off, so a plain revert was the only quick fix. It suggested any return should be an unstable, opt-in `Builder` setting.
5. **#8135** ("refactor the spawn blocking queue to make adding a new sharded implementation easy", by alex) was merged on 2026-07-31. It made the queue swappable, which set up #8337.

**Uncertainty:** I did not read #8337's diff. Its title and body support the opt-in claim, but I did not verify the Builder method name or the environment variable in the code at `facc6fc47e`. Separately, there is an open PR, #8532 ("rt: bound sharded blocking queue batches"), that builds on this queue. It is not part of the answer.