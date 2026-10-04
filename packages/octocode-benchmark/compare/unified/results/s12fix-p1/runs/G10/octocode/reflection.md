**1. Helped:**
- The first `ghGetHistoryItem` call (body, patches, comments) delivered the motivation, the two upstream Miri issues and most of the diff in one shot.
- `ghSearchCode` with keyword `miri` filtered to `tcp_*` under `tokio/tests` gave the explicit ignore reasons (SO_LINGER, keepalive, nodelay, etc.) in a compact form.
- The batched `ghGetHistoryItem` call with a `files` filter pulled the exact patches for `tcp_echo`, `tcp_socket`, `rt_common` and `rt_threaded`.

**2. Did not help:**
- The `files: ["tokio/tests/"]` call only returned the file list I already had, so it was largely redundant.
- The `ghSearchCode` call for `cfg_attr(miri, ignore)` returned a lot of unrelated fs and sync matches.
- Patches were truncated: the `tcp_stream.rs` patch was cut off and 7 changed files on page 2 were never fetched. I never followed `next.continuePatch` or `nextChangedFilesPage`.
- Code search runs on the default branch (`5d5cd8b5`), not the PR merge commit, so it mixes post-merge state with the PR diff.

**3. Next time:**
- Follow the pagination hints for the remaining patches.
- Read `tcp_accept_ready.rs` and `buffered.rs` directly with `ghGetFileContent` to confirm why they're still excluded.
- Avoid broad searches and ask for specific files instead.

**4. Confidence:** Medium-high on what the PR changes and on the listed ignore reasons, since I saw them in diffs and search output. Medium on completeness of the "still not run" list, because of the unread patches and the two unverified files.