1. **Helped:** The first call did the most. It ran `git rev-parse HEAD` to confirm the pinned commit, then used `rg` to find `getGenericCommand` and `expireIfNeeded` with line numbers. The second call printed `getGenericCommand` and the `expireIfNeeded` body. The third printed `lookupKey`, `lookupKeyReadOrReply`, and the start of `deleteKeyAndPropagate`. Together these gave the whole path from command to lookup, expiry check, delete and propagate.

2. **Did not help:**
   - The `rg` in my second call printed `deleteExpiredKeyAndPropagate` but cut off before the delete body.
   - The fourth call reprinted `deleteKeyAndPropagate` lines I had already partly seen, so I needed a fifth call to finish it.
   - I never read `confAllowsExpireDel`. I only saw its tail, and I said so in the answer.
   - No tool errors.

3. **Next time:** I'd print fixed line ranges around each function in one call, such as `sed -n 2847,2900p`. I'd also add `confAllowsExpireDel` to that call.

4. **Confidence:** High on the core behavior: GET returns nil, and a master lazily deletes the key and propagates a DEL. I read the source directly, and the line numbers come from the printed output. Medium on the exact line citations for the `expireIfNeeded` sub-branches, because I computed them from the printed range. I also did not run anything to confirm.