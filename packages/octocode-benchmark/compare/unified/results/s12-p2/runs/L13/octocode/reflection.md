**1. Helped**
- The first `localSearch` over `src/` for the function names (`getGenericCommand`, `expireIfNeeded`, etc.) located `getCommand` at `src/t_string.c:456-470` in one call.
- Parallel `localFetch` ranges of `t_string.c` and a `localSearch` of `db.c` gave the call chain and the `expireIfNeeded` line numbers.
- A single `localFetch` with three ranges of `db.c` returned `lookupKey` and `expireIfNeeded`, which held all the deciding branches.

**2. Did not help**
- I passed `regex: "rust"` in the first `localSearch`. That was a malformed parameter, not a real option. The search still ran, but I should not have sent it.
- The `localFetch` range output elided lines 371-2894 and 2926-2984, as expected.
- No errors occurred.

**3. Next time**
- Add one `localSearch` or `localFetch` for `lookupKeyReadOrReply` in `db.c`. I skipped it and inferred that it calls `lookupKey`. I disclosed that in my answer, but it was an avoidable gap.
- Open the `keyIsExpired` definition.
- Check the tests for GET on an expired key.
- Use the correct search parameters.

**4. Confidence**
Medium-high. The `lookupKey` and `expireIfNeeded` behavior is directly read, with line numbers. The one unverified link is `lookupKeyReadOrReply`, and I did not check which flags it passes.