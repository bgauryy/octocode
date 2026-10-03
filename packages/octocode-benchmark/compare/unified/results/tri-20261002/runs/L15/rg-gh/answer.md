Guava's immutable hash sets detect hash flooding while the set is being built. If they see it, they switch to a `java.util.HashSet`-backed implementation. I read `ImmutableSet.java` in `guava/src/com/google/common/collect/`. I did not check whether other construction paths, such as `copyOf` or `RegularImmutableSet`, use the same builder. I also did not read all of `hashFloodingDetected`'s body.

- **Normal case:** `RegularSetBuilderImpl` builds an open-addressed, linear-probing table. The class comment says it "attempts to detect hash flooding, and if it's identified, falls back to JdkBackedSetBuilderImpl" (`ImmutableSet.java:659-665`).

- **Check on every insert:** `insertInHashTable` (`:703-726`) probes at most `maxRunBeforeFallback` slots, starting from the smeared hash (`Hashing.smear`). If it finds no empty slot or equal element within that run, it returns `new JdkBackedSetBuilderImpl<E>(this).add(e)` (`:725`). This catches many identical or colliding hash codes, which would otherwise make construction O(n²).

- **Run limit:** `maxRunBeforeFallback(tableSize)` is `MAX_RUN_MULTIPLIER * log2(tableSize)` (`:878-879`). `MAX_RUN_MULTIPLIER = 13` (`:815`). The comment says it was "determined experimentally" to keep the false-positive rate low (`:808-815`).

- **Check at build time:** `review()` calls `hashFloodingDetected(hashTable)` and, if it returns true, switches to the JDK-backed builder (`:744`). The Javadoc at `:817-834` explains why this second check exists:
  - The per-insert check misses hash codes crafted to land in ascending table slots. Construction stays O(n), but later queries could be O(n).
  - The whole-table scan is O(n) worst case and O(n / log n) on average.
  - If it returns false, no query can take more than O(log n).

- **Fallback:** `JdkBackedSetBuilderImpl` (`:883-935`) keeps a `HashSet` delegate, which the comment says has "built in hash flooding protection" (`:884-885`). Its `build()` returns a `JdkBackedImmutableSet` wrapping that delegate and the deduplicated element list (`:931`).

- **Builder entry point:** `ImmutableSet.Builder` can also switch to `JdkBackedSetBuilderImpl` directly, at `:446`. I did not read the context there, so I can't say when it does so.