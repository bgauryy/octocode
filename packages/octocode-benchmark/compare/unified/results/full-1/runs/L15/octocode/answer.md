**Short answer:** `ImmutableSet` builds an open-addressed, linear-probing table. While building, it watches for long runs of filled slots. If it sees one, it abandons its own table and switches to a `java.util.HashSet`-backed set, whose own flooding protection is the JDK's treed bins. All file paths below are under `guava/src/com/google/common/collect/`.

**Mechanism** (`ImmutableSet.java`, `RegularSetBuilderImpl`):
1. **Online check on every insert.** `insertInHashTable` (around line 710–725) probes at most `maxRunBeforeFallback` slots from the smeared hash (`for (i = i0; i - i0 < maxRunBeforeFallback; i++)`). If it never finds an empty slot or an equal element, the loop ends and it runs `return new JdkBackedSetBuilderImpl<E>(this).add(e)` (line 725). This catches many exactly colliding hashes, which would otherwise make construction O(n²).
2. **Run-length threshold.** `maxRunBeforeFallback(tableSize)` is `MAX_RUN_MULTIPLIER * log2(tableSize)`, with `MAX_RUN_MULTIPLIER = 13`. The code comments say this value was found experimentally to keep false positives low.
3. **Whole-table check in `review()`.** `review()` returns `new JdkBackedSetBuilderImpl<E>(this)` if `hashFloodingDetected(hashTable)` is true (line 744). `hashFloodingDetected` (line 835) scans the whole table for a run longer than the threshold. Its Javadoc says this catches hashes adversarially chosen to land in ascending slots. That pattern keeps construction O(n) but would make later queries O(n).
   - If it returns false, no query takes more than O(log n).
   - It is O(n) worst case and O(n / log n) on average, because it skips stretches that can't hold a long run.
   - It can return true on truly random input, but the Javadoc says `ImmutableSetTest` checks that this is unlikely.
4. **The fallback.** `JdkBackedSetBuilderImpl`'s Javadoc says it uses a JDK `HashSet`, "which has built in hash flooding protection". Its `build()` produces `new JdkBackedImmutableSet<>(delegate, asImmutableList(dedupedElements, distinct))`, with the constructor at `JdkBackedImmutableSet.java:33`. That class keeps the `HashSet` for `contains` and an `ImmutableList` for ordering.

**Related, not sets:** the immutable maps and multisets use a similar idea with a bucket-length cap instead of a run-length cap.
- `RegularImmutableMap.MAX_HASH_BUCKET_LENGTH = 8` (line 70, checked at line 250) and `RegularImmutableBiMap` (line 142) use it.
- `RegularImmutableMultiset.MAX_HASH_BUCKET_LENGTH = 9` (line 119, checked at line 94) uses it too.

I saw only the constants and their check sites for these, not what they do on overflow.

**Uncertainty:** I read the code from line 640 onward, and I got the line numbers for `insertInHashTable` and the class bodies by counting, not from a search hit. Lines 725, 744 and 835 come directly from search results. I did not read `JdkBackedImmutableSet` beyond its constructor line.