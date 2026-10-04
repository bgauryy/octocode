1. **Helped:** The second Bash call, a `grep -rnE` over four named files in `guava/src/com/google/common/collect/` for flooding-related identifiers, found the relevant code in `ImmutableSet.java` at once. The third call, a `sed -n` over lines 660–760 and 800–935 of that file, gave the full mechanism and the comments explaining it. Both calls ran against the local checkout, which was fast.

2. **Did not help:**
   - The first call used `rg`, which isn't installed, so it was wasted.
   - Every Bash call printed `/dev/null: Operation not permitted`. That was noise, and it made `git log -1` fail, so I never confirmed the checkout was at the pinned SHA. My answer assumes the checkout matches 4d41665.
   - I never opened `JdkBackedImmutableSet`, `ImmutableSetTest` or the `ImmutableMap` detection logic. My claims about them are inferred from call sites and comments.

3. **Next time:** I would use `grep` from the start. I'd also check the pinned commit another way, for example by reading `.git/HEAD` directly. And I'd add one more read of `JdkBackedImmutableSet.java` and the test file.

4. **Confidence:** High for the core mechanism, because I read the code and the line numbers. Medium for the claims about the false-positive tests and `ImmutableMap`, which I didn't open, and for the commit match.