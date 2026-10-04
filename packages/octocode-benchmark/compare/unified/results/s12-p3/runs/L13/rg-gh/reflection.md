1. **Helped:** The first `grep -n` over `src/t_string.c` and `src/db.c` found `getGenericCommand`, `lookupKeyReadOrReply` and `expireIfNeeded` in one call. The next `sed -n` over `src/db.c` printed `lookupKey` and the full `expireIfNeeded` body, which settled the master, replica and paused cases. The third call covered the miss path and `deleteExpiredKeyAndPropagate`.

2. **Did not help:**
   - Every Bash call printed `/dev/null: Operation not permitted`.
   - My first call, `git log -1 --format=%H`, failed with a `/dev/null` fatal error, so the SHA never printed. I cited the pinned commit in the answer anyway, but I never confirmed the checkout was at `20bb2cfc54`.
   - I never opened `deleteKeyAndPropagate`. The `expired` event and the DEL/UNLINK propagation are inferred from its call site and header comments.

3. **Next time:** I would read the SHA from `.git/HEAD` or `git rev-parse HEAD` with stderr handled, and say so if that failed. I would also `sed` the body of `deleteKeyAndPropagate` instead of inferring it.

4. **Confidence:** High on the GET path and the replica and master branching, since I read that code directly. Medium on the commit pin, because I never verified it, and on the propagation details, because I never read that function.