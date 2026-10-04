**Answer:** tokio-rs/tokio PR #8337, "runtime: add an opt-in sharded spawn_blocking queue", added the opt-in. The earlier attempt, PR #7757, was merged and then reverted.

**The opt-in: PR #8337**
- It was authored by alex and merged on 2026-08-09 (merge commit `8b13642a1f7346814f189937b2d6e1e80b210db1`).
- Its body says it "Re-lands the sharded queue from #7757 (reverted due to #8056), disabled by default."
- You turn it on with the unstable `Builder::enable_sharded_blocking_queue` or the `TOKIO_UNSTABLE_SHARDED_BLOCKING_QUEUE` environment variable.
- PR #8135, merged on 2026-07-31, refactored the `spawn_blocking` queue so a sharded implementation could be added easily. I inferred from the dates and titles that #8337 builds on it. I didn't read #8337's diff or discussion to confirm.

**The earlier attempt: PR #7757, merged and then reverted**
- PR #7757, "rt: improve spawn_blocking scalability with sharded queue", was by alex and merged on 2026-04-10.
- It shipped in 1.52.0. PR #8045 is the "prepare 1.52.0" chore, merged on 2026-04-14. I didn't open it to confirm #7757 is in the release.
- It caused a regression where programs using `spawn_blocking` hung (issue #8056).
- hawkw's PR #8057, "rt: revert #7757 to fix regression in `spawn_blocking`", merged on 2026-04-16 (merge commit `56aaa43e91c4fbed88f0c2a5b65019ed9a0c3c61`). The PR body says a v1.52.1 release was needed as soon as possible.
- The body also says the hang still happened with `NUM_SHARDS` set to 1, so the sharding couldn't simply be switched off.
- It suggests that any future sharded queue should start as an unstable, opt-in `Builder` setting.

**Uncertainty:** I didn't confirm that `facc6fc47e` contains #8337's merge commit. I assumed it does because the question says the opt-in exists at that commit. Open PR #8532, "rt: bound sharded blocking queue batches", is a follow-up and is not merged.