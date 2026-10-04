**Answer:** PR #8337, "runtime: add an opt-in sharded spawn_blocking queue", added the opt-in. It was merged on 2026-08-09. The earlier attempt, #7757, was merged and then reverted.

**Evidence** (from GitHub search results):
- **#7757**, "rt: improve spawn_blocking scalability with sharded queue", was merged on 2026-04-10.
- **#8056** reported a regression caused by #7757, where tests hung in `spawn_blocking`.
- **#8057**, "rt: revert #7757 to fix regression in `spawn_blocking`", was merged on 2026-04-16. The revert went out with the 1.52.0 prep, #8045, merged 2026-04-14.
- **#8067**, "Re-land sharded spawn blocking queue", is still open and was not merged.
- **#8527**, "Sharded spawn_blocking queue can leave later shards waiting under load", is open. It is a follow-up issue on the sharded queue.

**Uncertainty:**
- I did not read the source at facc6fc47e. I also did not confirm that #8337 is an ancestor of that commit, though its August merge date makes that very likely. The unstable flag name is unchecked.
- The PR titles, states and dates come from search metadata only. I did not read the PR bodies.
- The reason for the revert comes from #8056 and #8057's titles, not their contents.