**Helped:**
- The two parallel `ghSearchHistory` pull-request queries ("unix socket" with "scrape", and "unix" with "socket") surfaced both #18091 and #19399 in one call.
- One batched `ghGetHistoryItem` call with `include: ["body"]` for both PRs gave the issue (#12024), merge SHAs and the mix-up explanation. Two calls total got the answer.

**Did not help:**
- The second search query was largely noise: renovate bumps, Graphite remote storage, and Docker Swarm.
- I never verified that ea954809ce actually contains the two merge commits. I assumed it from the August 2026 merge dates and said so in the answer, but that part of the question is unconfirmed.
- I did not open issue #12024 to confirm it closed. The PR body even has a "Fixes Fixes" typo.
- I did not read the #19399 diff, so the mechanism comes only from the PR description.

**Next time:**
- Add a `ghGetHistoryItem` commit lookup or compare for ea954809ce to check ancestry.
- Fetch issue #12024.
- Use `include: ["files"]` on #19399, or `ghGetFileContent` at the pinned commit, to cite the actual client-caching lines.

**Confidence:** high on the PR numbers and the issue reference, since the PR bodies state them directly. Medium on "as of ea954809ce" and on the mechanism, for the reasons above.