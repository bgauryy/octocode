Guava's immutable hash sets detect hash flooding while they are being built. If they find it, they switch to a `java.util.HashSet`, which has its own flooding protection. All line numbers below are in `java/guava/src/com/google/common/collect/ImmutableSet.java`.

**How the set is built.** `ImmutableSet.Builder` uses `RegularSetBuilderImpl` (class doc at ~line 664), an open-addressed, linear-probing hash table. The class doc says it "attempts to detect hash flooding, and if it's identified, falls back to JdkBackedSetBuilderImpl."

**Two checks:**

1. **Online check on every insert** (`insertInHashTable`, ~lines 700–720):
   - The probe loop runs `for (int i = i0; i - i0 < maxRunBeforeFallback; i++)`.
   - If it finds no empty slot and no equal element within that many probes, it executes `return new JdkBackedSetBuilderImpl<E>(this).add(e);`.
   - This catches many elements with identical or clustered hash codes, which would otherwise make construction O(n²).
   - The run limit is `maxRunBeforeFallback(tableSize) = MAX_RUN_MULTIPLIER * log2(tableSize)` (~line 876), with `MAX_RUN_MULTIPLIER = 13` (~line 814). The source comment says 13 was "determined experimentally" to match the desired false-positive probability.

2. **Whole-table check when the builder is finalized.**
   - `review()` ends with `return hashFloodingDetected(hashTable) ? new JdkBackedSetBuilderImpl<E>(this) : this;` (~line 744).
   - `hashFloodingDetected` (~line 835) scans the table for any run of filled slots longer than the limit.
   - It exists because the online check can't catch hash codes adversarially designed to land in ascending table slots. That pattern keeps construction O(n) but can make queries O(n).
   - The javadoc says that if it returns false, no query takes more than O(log n). It runs in O(n) worst case and O(n/log n) on average, and may rarely report a false positive on random input.

**The fallback.** `JdkBackedSetBuilderImpl` (~line 882) copies the deduplicated elements into a `HashSet` made with `newHashSetWithExpectedSize`. Its doc says "uses a JDK HashSet, which has built in hash flooding protection." That protection is HashMap's tree bins, which I'm stating from general knowledge. I didn't read JDK code.

**Not verified.** I did not read the `RegularImmutableSet`/`JdkBackedImmutableSet` build path beyond `build()`, or the tests. `RegularImmutableMap` has a separate mechanism, `MAX_HASH_BUCKET_LENGTH = 8` (line 70), which I only saw in search hits and did not read. The same approach applies to maps, but I haven't confirmed the details.