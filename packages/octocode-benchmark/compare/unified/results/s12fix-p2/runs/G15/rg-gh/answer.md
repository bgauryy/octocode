**Answer:** PR #8337, "runtime: add an opt-in sharded spawn_blocking queue", added the opt-in. It was merged on 2026-08-09. The earlier attempt, #7757, was merged and then reverted.

- **Earlier attempt:** #7757, "rt: improve spawn_blocking scalability with sharded queue", was merged on 2026-04-10.
- **Regression:** issue #8056, "regression in `spawn_blocking` caused by #7757 causes tests to hang", reported the problem.
- **Revert:** PR #8057, "rt: revert #7757 to fix regression in `spawn_blocking`", was merged on 2026-04-16.
- **Re-land attempt:** PR #8067, "Re-land sharded spawn blocking queue", is still open and unmerged.
- **Open issue on the new opt-in:** #8527, "Sharded spawn_blocking queue can leave later shards waiting under load", is open.

**Not verified:** I found these through search results only. I didn't read the PR bodies or diffs. I also didn't confirm that #8337's merge commit is an ancestor of `facc6fc47e`. The commit message I saw for `facc6fc47e` is #8515, which is numbered earlier than #8527 but later than #8337. That makes the ancestry likely but not checked.