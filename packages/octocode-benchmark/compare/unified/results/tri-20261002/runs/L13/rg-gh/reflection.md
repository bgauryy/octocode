1. **Helped:** The first Bash call, an `rg -n` over `src/*.c` for the function definitions, located `getGenericCommand`, `expireIfNeeded` and `deleteExpiredKeyAndPropagate` in one step. The `sed -n` range reads of `src/t_string.c`, `src/db.c:285-360` and `src/db.c:2847-3090` then gave the full path with line numbers. Batching several `sed` reads into one call kept the call count low.

2. **Did not help:**
   - Every Bash call printed "/dev/null: Operation not permitted". The sandbox blocks it, which was noisy but harmless.
   - `git rev-parse HEAD` failed because of that, so I could not confirm the checkout was at 20bb2cf. I said so in the answer.
   - I never opened `lookupKeyRead`, `keyIsExpired` or `confAllowsExpireDel`. I assumed `lookupKeyRead` calls `lookupKey` and flagged that in the answer.
   - I cited line numbers such as `src/db.c:3059-3068` and `3036-3041` from a long `sed` output without re-checking them against a numbered listing. They could be slightly off.

3. **Next time:** Use `rg -n` or `sed -n` with `cat -n` so every cited line number is verified. Read `lookupKeyRead` directly. Try reading the commit from `.git/HEAD` to get around the `/dev/null` failure.

4. **Confidence:** Medium-high on the behavior, because I read the code that implements it. Medium on the exact line numbers and on the pinned commit, since I could not verify the checkout's SHA.