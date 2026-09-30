**Short answer:** `ImmutableSet`'s builder uses open-addressed linear probing. It watches for long runs of occupied slots, both while inserting and at the end. If it sees one, it drops the custom table and switches to a `java.util.HashSet`-backed implementation, `JdkBackedImmutableSet`. The JDK's `HashSet` has its own flooding protection. All line numbers below are in `guava/src/com/google/common/collect/ImmutableSet.java`.

**Mechanism**
1. **Online check while inserting.** `RegularSetBuilderImpl.insertInHashTable` (line 704) probes linearly from `Hashing.smear(hash)`. The loop is `for (i = i0; i - i0 < maxRunBeforeFallback; i++)` (line 711). If it probes that many slots without finding an empty slot or an equal element, it falls back with `new JdkBackedSetBuilderImpl<E>(this).add(e)` (line 725). This catches many identical or colliding hash codes, which would otherwise make construction O(n²).
2. **Threshold.** `maxRunBeforeFallback(tableSize) = MAX_RUN_MULTIPLIER * log2(tableSize)` (lines 878-879), with `MAX_RUN_MULTIPLIER = 13` (line 815). The comment at lines 809-815 says the value was "determined experimentally" to match the desired false-positive probability.
3. **Final check at build time.** `review()` calls `hashFloodingDetected(hashTable)` and switches to `JdkBackedSetBuilderImpl` if it returns true (line 744). The javadoc for `hashFloodingDetected` (starts around line 820, method at 835) explains why this is needed:
   - The online check can't catch hash codes designed to land in ascending table slots. That pattern keeps construction O(n) but can make later queries O(n).
   - The full-table scan takes O(n) worst case and O(n / log n) on average.
   - If it returns false, no query can take more than O(log n).
4. **The fallback itself.** `JdkBackedSetBuilderImpl` (line 886) keeps a `HashSet` delegate. `build()` returns `JdkBackedImmutableSet` (line 927). The class comment at line 665 says this fallback exists because the JDK `HashSet` "has built-in flooding protection".

**Uncertainty**
- I read `ImmutableSet.java` only.
- I saw `RegularImmutableMap.java:70` define `MAX_HASH_BUCKET_LENGTH = 8`, a bucket-length limit for maps. That is a separate mechanism, and I didn't trace it.
- I didn't read the body of `hashFloodingDetected` (lines 835-870) in detail. I only saw its javadoc and a few of its lines.