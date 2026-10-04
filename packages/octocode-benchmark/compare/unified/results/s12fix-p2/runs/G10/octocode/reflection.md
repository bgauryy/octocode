**Helped:** The first `ghGetHistoryItem` call (PR 8156 with body and patches) did most of the work. It returned the motivation, the two Miri blockers with links, and most of the diff. The three-row `ghSearchCode` call on `tokio/tests` was useful for the remaining ignores. Its `cfg(not(miri))` row and its `cfg_attr(miri, ignore)` + `socket` row listed the leftover UDS, UDP, quickack and tclass cases.

**Did not help:**
- The follow-up `ghSearchCode` call (readv, miri/issues, miri/pull) returned empty on all three rows. It was a wasted call.
- The first `ghSearchCode` row ("miri", "ignore", "rust-lang/miri") also came back empty.
- I never followed `next.continuePatch` or `nextChangedFilesPage`. The `tcp_*.rs`, `rt_threaded.rs` and `rt_handle_block_on.rs` diffs and file page 2 stayed unread. That left me unable to say which tests the `readv` leak affects.
- `ghSearchCode` searches the default branch, which is later than the PR, so I could not tell which ignores predate or postdate it.

**Next time:** I would call `ghGetHistoryItem` with the `continuePatch` files and `filePage: 2`, and filter on `matchString: "ignore"` to see exactly which tests the PR left ignored.

**Confidence:** Medium. The description of the PR's changes and the Miri blockers is well supported. The "still ignored" list is incomplete, and it comes from the default branch rather than the PR commit.