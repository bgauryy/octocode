**Answer:** Guava's `ImmutableSet` uses open-addressed linear probing. It limits how long a probe run may get. If a run gets too long, it falls back to a JDK `HashSet`-backed implementation, which has its own flooding protection. This is in `guava/src/com/google/common/collect/ImmutableSet.java`. I didn't open `JdkBackedImmutableSet` itself, so what it does is inferred from the builder code.

**Mechanism**
- **Online check while adding** (`RegularSetBuilderImpl`, declared around line 666):
  - `insertInHashTable` probes at most `maxRunBeforeFallback` slots, starting from `Hashing.smear(hashCode)` (around lines 713–735).
  - If the loop ends without finding a free slot or an equal element, it returns `new JdkBackedSetBuilderImpl<E>(this).add(e)` (the "fell out of the loop due to a long run" comment).
  - This catches many identical or colliding hashes, which would otherwise make construction O(n²), per the doc at lines 821–824.
- **Run limit** (lines 808–815, 875–880):
  - `maxRunBeforeFallback(tableSize)` is `MAX_RUN_MULTIPLIER * log2(tableSize)`, with `MAX_RUN_MULTIPLIER = 13`.
  - The comment says 13 was determined experimentally to keep false positives low.
- **Whole-table check at build time** (`review()`, line 744, and `hashFloodingDetected`, line 835):
  - `review()` returns a `JdkBackedSetBuilderImpl` if `hashFloodingDetected(hashTable)` is true.
  - This scans for any run of filled slots longer than the limit.
  - It catches hash codes designed to land in ascending slots. Those keep construction O(n) but could make later queries O(n) (doc at lines 826–829).
  - If it returns false, no query takes more than O(log n) (doc at lines 830–831).
  - It takes O(n) in the worst case and O(n / log n) on average, because it skips ahead where the table is empty.
- **Fallback type** (`JdkBackedSetBuilderImpl`, lines ~884–935):
  - It keeps a `java.util.HashSet` delegate, created with `newHashSetWithExpectedSize`.
  - It records insertion order in `dedupedElements`.
  - Its `build()` returns `new JdkBackedImmutableSet<>(delegate, asImmutableList(...))`.

**Related:** `ImmutableMap` has the same pattern. `RegularImmutableMap.java:101` calls `JdkBackedImmutableMap.create(...)`. I only saw that call site, not the surrounding detection logic.

**Uncertainty:** The false-positive claim ("tests that the probability is low") comes from a comment that points to `ImmutableSetTest`. I didn't open that test.