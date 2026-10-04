**Answer:** PR #8337, "runtime: add an opt-in sharded spawn_blocking queue", added the opt-in. It was merged on 2026-08-09. The earlier attempt to shard the queue was PR #7757. It was merged, then reverted.

- **#7757** ("rt: improve spawn_blocking scalability with sharded queue") was merged on 2026-04-10.
- **Regression:** Issue #8056 reported that #7757 caused a `spawn_blocking` regression that made tests hang.
- **Revert:** PR #8057 ("rt: revert #7757 to fix regression in `spawn_blocking`") was merged on 2026-04-16.
- **Re-land:** Issue/PR #8067 ("Re-land sharded spawn blocking queue") is still open. I didn't check whether it is an issue or a PR.
- **Open issue after #8337:** Issue #8527 ("Sharded spawn_blocking queue can leave later shards waiting under load") is open.

**Not verified:**
- I did not confirm that #8337 is an ancestor of `facc6fc47e`. The commit at that SHA is "signal: replace `Storage::for_each` with `Storage::iter` (#8515)". #8515 is numbered below #8527, so it probably comes after #8337's merge, but I only inferred that.
- I did not read the diffs, so I haven't seen the unstable flag or the `file:line` where the opt-in is defined.