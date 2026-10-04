**Answer:** PR #8337, "runtime: add an opt-in sharded spawn_blocking queue", added the opt-in. The earlier attempt, PR #7757, was merged and then reverted.

- **#8337** was authored by alex and merged on 2026-08-09 as commit `8b13642a1f73`. It builds on #8135, "refactor the spawn blocking queue to make adding a new sharded implementation easy". That refactor was merged on 2026-07-31 as commit `108d6d3dc038`.
- **#7757**, "rt: improve `spawn_blocking` scalability with sharded queue", was also by alex. It was merged on 2026-04-10 as commit `1604bc335157`.
  - **Revert:** hawkw reverted it in PR #8057, "rt: revert #7757 to fix regression in `spawn_blocking`". That PR was merged on 2026-04-16 as commit `56aaa43e91c4`.
  - **Release timing:** the 1.52.0 release-prep PR #8045 was merged on 2026-04-14, between the two, as commit `5f7be0ac42cb`. I didn't check whether #7757 shipped in 1.52.0.

**Not verified:**
- I only read PR titles, dates and merge commits from the search results. I didn't open the PR bodies or diffs, so I haven't confirmed the stated regression or how the new implementation is gated.
- I didn't check that `facc6fc47e` contains #8337. Its merge date (2026-08-09) is just earlier than the open PR #8532, "rt: bound sharded blocking queue batches" (2026-10-02), so I assumed it does.
- PR #7973, a separate closed PR sharding the multi-thread inject queue, is not about the blocking queue.