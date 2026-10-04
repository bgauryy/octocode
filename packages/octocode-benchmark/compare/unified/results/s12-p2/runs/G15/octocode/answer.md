The opt-in was added by **PR #8337**, "runtime: add an opt-in sharded spawn_blocking queue". Alex authored it, and it merged on 2026-08-09. I didn't open the PR body or diff. This comes from PR search metadata (titles, states and dates), and I didn't check that facc6fc47e includes the merge.

The earlier attempt to shard the queue was **PR #7757**, "rt: improve spawn_blocking scalability with sharded queue", also by Alex. It merged on 2026-04-10 and was later reverted:
- **Revert:** PR #8057, "rt: revert #7757 to fix regression in `spawn_blocking`", merged on 2026-04-16 by hawkw. The title gives the reason as a `spawn_blocking` regression, but I didn't read the PR to confirm the details.
- **Follow-up:** PR #8135 refactored the blocking queue so a sharded implementation could be added more easily. It merged on 2026-07-31. #8337 then added the sharded queue as an opt-in on top of that.

A separate Alex PR, #7973, tried to shard the multi-thread inject queue. It was closed without merging. It concerns a different queue, not the `spawn_blocking` one.

I'm confident in the PR numbers, titles and dates. The reasoning behind the revert is inferred from the PR title only.