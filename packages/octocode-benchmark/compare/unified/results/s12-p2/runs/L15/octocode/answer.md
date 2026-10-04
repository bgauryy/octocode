**Short answer:** `ImmutableSet`'s builder uses open-addressed linear probing with a cap on how long a probe run can get. If it detects a long run, which suggests hash flooding, it abandons its own table. It switches to a wrapper around `java.util.HashSet`, which Guava's comments say "has built in hash flooding protection". Everything below is in `guava/src/com/google/common/collect/ImmutableSet.java`, inside `RegularSetBuilderImpl` and `JdkBackedSetBuilderImpl`.

1. **Online check on each insert** (`insertInHashTable`, lines 706–726).
   - The element's hash is spread with `Hashing.smear`, then the table is probed linearly.
   - The loop runs only while `i - i0 < maxRunBeforeFallback` (line 711).
   - If it falls out of the loop, the run was too long. The code then returns `new JdkBackedSetBuilderImpl<E>(this).add(e)` (line 725).
   - The `ImmutableSet.java` comment on `hashFloodingDetected` (lines 821–823) says this check catches things like many exactly matching hash codes, which would otherwise make construction O(n²).

2. **The run limit** (`maxRunBeforeFallback`, lines 878–880). It is `MAX_RUN_MULTIPLIER * log2(tableSize)`, with `MAX_RUN_MULTIPLIER = 13` (line 815). The comment says the value was chosen experimentally to keep false positives low (lines 807–814). The limit is recomputed whenever the table is resized (lines 741 and 803).

3. **Whole-table check at the end** (`review()` at line 744, `hashFloodingDetected` at lines 835–871).
   - It scans the finished table for runs of non-null slots longer than the limit, in O(n) worst case and O(n / log n) on average. It skips ahead where it can and handles wrap-around.
   - The online check misses hash codes crafted to land in ascending table slots. That keeps construction O(n) but can still make later queries O(n). This scan catches that case, and the comment says that if it returns false, no query takes more than O(log n) (lines 821–826).
   - If flooding is detected, `review()` returns `new JdkBackedSetBuilderImpl<E>(this)`.

4. **The fallback** (`JdkBackedSetBuilderImpl`, lines 886–899). It copies the deduplicated elements into a JDK `HashSet` created with `newHashSetWithExpectedSize`. `HashSet` is described in the code comment as having built-in flooding protection.

**Caveats**
- I read only `ImmutableSet.java`.
- `ImmutableMap` has a related defense in `RegularImmutableMap.java`: `MAX_HASH_BUCKET_LENGTH = 8` (line 70), checked at line 250. I only saw those two lines from a search and did not read the surrounding code, so I haven't described its behavior.
- I did not read the rest of `JdkBackedSetBuilderImpl` (past line 900) or how `build()` works for that case.