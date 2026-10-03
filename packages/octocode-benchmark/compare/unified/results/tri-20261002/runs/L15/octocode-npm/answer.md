**Answer:** `ImmutableSet` builds a linear-probing hash table and watches for abnormally long runs of filled slots. If it sees one, it abandons its own table and switches to a wrapper around `java.util.HashSet`, which has its own flooding protection. All of this is in `guava/src/com/google/common/collect/ImmutableSet.java` (`RegularSetBuilderImpl` and `JdkBackedSetBuilderImpl`).

**Evidence:**

1. **Online check while adding** (`insertInHashTable`, lines 706–726).
   - Each element is probed from `Hashing.smear(hashCode)`, with the loop bounded by `i - i0 < maxRunBeforeFallback` (line 711).
   - If the loop ends without finding an empty slot or an equal element, the comment says "we fell out of the loop due to a long run; fall back to JDK impl" (line 724).
   - It then returns `new JdkBackedSetBuilderImpl<E>(this).add(e)` (line 725).
   - This catches many elements with exactly matching hash codes, which would otherwise make construction O(n²) (javadoc, lines 821–823).

2. **Run-length threshold** (lines 815 and 878–880).
   - `maxRunBeforeFallback(tableSize) = MAX_RUN_MULTIPLIER * log2(tableSize)`, with `MAX_RUN_MULTIPLIER = 13`.
   - The comment at lines 812–814 says the value was chosen experimentally to keep false positives low.

3. **Whole-table check at the end** (`review()`, line 744, calling `hashFloodingDetected`, lines 835–871).
   - After building, `review()` returns `new JdkBackedSetBuilderImpl<E>(this)` if `hashFloodingDetected(hashTable)` is true.
   - This scan looks for any run of filled slots longer than `maxRunBeforeFallback`. It skips ahead over stretches that cannot contain such a run, so it takes O(n) in the worst case and O(n / log n) on average (javadoc, lines 817–819).
   - It exists because the online check cannot catch hash codes designed to land in ascending table slots. Those keep construction O(n) but could make later lookups O(n) (lines 821–824).
   - If it returns false, "no query can take more than O(log n)" (line 826).
   - It can return true on truly random input, but the javadoc says `ImmutableSetTest` tests that this is unlikely (lines 832–833).

4. **The fallback** is `JdkBackedSetBuilderImpl` (line 886).
   - It copies the deduplicated elements into a JDK hash set via `newHashSetWithExpectedSize(distinct)` (line 891).
   - The javadoc at lines 884–885 says this is because the JDK set "has built in hash flooding protection".
   - I did not read lines 901–944, so I haven't checked how `build()` turns that delegate into the final set.

**Related (not `ImmutableSet`):**
- `RegularImmutableMultiset` uses chained buckets and falls back to `JdkBackedImmutableMultiset` when a bucket exceeds `MAX_HASH_BUCKET_LENGTH = 9` (lines 83–97 and 119).
- `RegularImmutableMap` throws `BucketOverflowException` when a bucket exceeds 8 entries (lines 70 and 250). I did not read what catches it.
- `CompactHashSet` converts to a flooding-resistant implementation at bucket length 9 (lines 143 and 336).
- `CompactHashMap` converts the same way (lines 127 and 379).