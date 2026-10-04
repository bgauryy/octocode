Guava's `ImmutableSet` builder detects hash flooding and, when it does, falls back to a `java.util.HashSet`-backed implementation. Per the comments, the JDK set has built-in flooding protection. All lines below are in `guava/src/com/google/common/collect/ImmutableSet.java`. I only read the builder code there and did not read `RegularImmutableSet` or `JdkBackedImmutableSet` themselves.

**Default implementation.** `RegularSetBuilderImpl` (line 667) builds an open-addressed, linear-probing table. The class comment at lines 664-665 says it detects hash flooding and then falls back to `JdkBackedSetBuilderImpl`.

**Two detection checks:**

1. **Online probe-run limit while inserting.**
   - `insertInHashTable` (lines 706-726) probes at most `maxRunBeforeFallback` slots.
   - If it never finds an empty slot or an equal element, it returns `new JdkBackedSetBuilderImpl<E>(this).add(e)` (line 725).
   - The limit is `maxRunBeforeFallback(tableSize) = MAX_RUN_MULTIPLIER * log2(tableSize)` (lines 878-880), with `MAX_RUN_MULTIPLIER = 13` (line 815).
   - The comment says the multiplier was "determined experimentally" to match the desired false-positive probability. A higher value slows worst-case `contains`, speeds up `hashFloodingDetected` and reduces false positives (lines 807-815).
   - This catches many identical hash codes, which would otherwise make construction O(n²) (lines 821-823).

2. **Whole-table scan at `review()`.**
   - `review()` (lines 734-745) returns a `JdkBackedSetBuilderImpl` if `hashFloodingDetected(hashTable)` is true (line 744).
   - `hashFloodingDetected` (lines 835-871) scans for any run of filled slots longer than `maxRunBeforeFallback`, including runs that wrap around the end of the table.
   - It skips ahead when it can, so it takes O(n) in the worst case and O(n / log n) on average (lines 817-818).
   - It catches adversarial hash codes that map to ascending table slots. Those keep construction O(n) but could make later queries O(n) (lines 821-825).
   - If it returns false, no query can take more than O(log n) (line 826).

**The fallback.**
- `JdkBackedSetBuilderImpl` (lines 886-930) copies the deduplicated elements into a JDK hash set (lines 889-898).
- For two or more elements, `build()` returns `new JdkBackedImmutableSet<>(delegate, asImmutableList(dedupedElements, distinct))` (line 927).

**Also in the checkout.** `RegularImmutableMap.java:70` defines `MAX_HASH_BUCKET_LENGTH = 8`, and line 250 checks `++bucketSize > MAX_HASH_BUCKET_LENGTH`. This looks like the analogous check for maps, but I did not read it beyond those two lines.

**Uncertainty.** Detection is probabilistic and can fire on truly random input. The comment at lines 832-833 says `ImmutableSetTest` checks that this is unlikely. I did not look at that test.