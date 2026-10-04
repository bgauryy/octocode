Guava's immutable hash sets detect likely hash flooding while building and, if they find it, switch to a `java.util.HashSet`-backed implementation. The code is in `guava/src/com/google/common/collect/ImmutableSet.java`. I read it directly and did not run any of it.

**1. Online detection while adding (`RegularSetBuilderImpl.insertInHashTable`, lines 706-726)**
- The table uses linear probing. Each element's hash is first spread with `Hashing.smear(eHash)` (line 709).
- The probe loop is capped: `for (int i = i0; i - i0 < maxRunBeforeFallback; i++)` (line 711).
- If the loop ends without finding an empty slot or an equal element, the builder switches over. Line 724 says "we fell out of the loop due to a long run; fall back to JDK impl", and line 725 returns `new JdkBackedSetBuilderImpl<E>(this).add(e)`.
- The cap is `maxRunBeforeFallback(tableSize) = MAX_RUN_MULTIPLIER * log2(tableSize)` (lines 878-880). `MAX_RUN_MULTIPLIER` is 13 (line 815). A comment calls it "surprisingly high" but says experiments required it, and that it was tuned to keep false positives low (lines 807-814).
- The cap is recomputed whenever the table is resized (line 803, and line 741 in `review()`).

**2. Whole-table check in `review()` (line 744)**
- `review()` calls `hashFloodingDetected(hashTable)` and returns a `JdkBackedSetBuilderImpl` if it reports flooding.
- `hashFloodingDetected` is defined at lines 835-871. It scans for runs of consecutive filled slots longer than `maxRunBeforeFallback`, and skips ahead where a run can't be long enough.
- The javadoc (lines 817-834) explains why this second check exists. The online check catches many identical hash codes, which would make construction O(n²). It can't catch hash codes crafted to land in ascending table slots. Those keep construction O(n) but make later queries O(n).
- If the scan returns false, no query can take more than O(log n). It can return true on random input, but the javadoc says `ImmutableSetTest` checks that this is unlikely.

**3. The fallback (`JdkBackedSetBuilderImpl`, line 886)**
- Its javadoc says it uses a JDK `HashSet`, "which has built in hash flooding protection". It copies the already-deduplicated elements into `newHashSetWithExpectedSize(distinct)` (lines 889-898).
- Line 446 also switches to it, in what appears to be the `Builder` copy-on-write path. I did not read that context.

**Related code**
- `RegularImmutableMultiset` uses the same approach with chained buckets. `MAX_HASH_BUCKET_LENGTH = 9` (line 119), `hashFloodingDetected` is at line 89, and it falls back to `JdkBackedImmutableMultiset` (lines 83-84).
- `RegularImmutableMap` has `MAX_HASH_BUCKET_LENGTH = 8` and throws `BucketOverflowException` (lines 70 and 250-251).
- `CompactHashSet` and `CompactHashMap` have a bucket limit of 9. They call `convertToHashFloodingResistantImplementation()` at lines 336-337 and 379-380 respectively.

**Not verified:** I skipped lines 751-794, which cover `build()` and the helper methods, so I haven't seen how `build()` creates the final set or whether it re-checks for flooding. I also didn't read the rest of `JdkBackedSetBuilderImpl` (past line 900) or the final-set classes such as `JdkBackedImmutableSet`.