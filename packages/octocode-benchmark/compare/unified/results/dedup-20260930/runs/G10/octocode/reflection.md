**1. Helped:**
- The first `ghGetHistoryItem` call (PR body plus `changedFiles`) gave the motivation and the two Miri issue and PR links (#5047 and #5054) right away.
- The second call, with `matchString: "miri"`, `matchContext: 1` and `patches.mode: "all"`, returned only the Miri-related hunks across 36 files. That covered nearly everything in one request.

**2. Did not help:**
- In the second call, the last nine files came back truncated with `patchPagination` and `hasMore`. `tcp_socket.rs` was among them and needed a third call.
- The third call still cut off `tcp_socket.rs` hunks, for example the `tclass_v6` and TOS changes.
- The call has no way to see the merged file, so I never checked final line numbers.
- I never checked whether #5047 and #5054 have landed. I also never checked why `resolve_dns` could be un-ignored.
- I didn't read the PR's comments or reviews.

**3. Next time:**
- Pass `fileFilter.paths` for `tokio/tests/tcp_*` in the second call, so the truncated files come back in one pass.
- Read `tcp_socket.rs` at `mergeCommitSha` with `ghGetFileContent` and `matchString: "miri"` to get real line numbers.
- Query the Miri repo for the status of #5047 and #5054.

**4. Confidence:**
- Medium-high. The list of remaining exclusions and their stated reasons is quoted directly from the diff, with line numbers omitted.
- The "still not run" list is only as complete as my "miri" filter.
- Unread truncated hunks and unchecked Miri issue status are the main gaps.