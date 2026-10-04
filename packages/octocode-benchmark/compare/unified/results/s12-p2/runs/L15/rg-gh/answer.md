**Answer:** Guava's `ImmutableSet` uses open-addressed linear probing. It detects hash flooding while building the set. If it finds a suspiciously long run of filled slots, it abandons its own table and uses a `java.util.HashSet`-backed set (`JdkBackedImmutableSet`). The JDK's `HashSet` has its own flooding protection. All references below are in `guava/src/com/google/common/collect/ImmutableSet.java`.

**Evidence:**

- **Online detection on insert.** `RegularSetBuilderImpl` (line ~666) javadoc says it "attempts to detect hash flooding, and if it's identified, falls back to JdkBackedSetBuilderImpl" (lines 664-665).
  - `insertInHashTable` (lines ~704-724) probes with `for (int i = i0; i - i0 < maxRunBeforeFallback; i++)`.
  - If the loop ends without finding a null slot or an equal element, the comment reads "we fell out of the loop due to a long run; fall back to JDK impl". It then returns `new JdkBackedSetBuilderImpl<E>(this).add(e)`.
  - The hash is first spread with `Hashing.smear(eHash)`.

- **Run-length threshold.** `maxRunBeforeFallback(tableSize)` (line ~878) returns `MAX_RUN_MULTIPLIER * log2(tableSize)`. `MAX_RUN_MULTIPLIER = 13` (line 815), and the comment says it was "determined experimentally to match our desired probability of false positives" (lines 808-811).

- **Whole-table check.** `review()` (lines ~735-745) returns `new JdkBackedSetBuilderImpl<E>(this)` if `hashFloodingDetected(hashTable)` is true. That method (line ~835) scans the table in roughly O(n / log n) on average and O(n) in the worst case, looking for any run of non-null slots longer than `maxRunBeforeFallback`.
  - The javadoc (lines 821-826) explains why this second check is needed. The online check catches many identical hash codes, which would make construction O(n²). It can't catch hash codes designed to land in ascending slots, which keep construction O(n) but make later queries O(n).
  - If the method returns false, "no query can take more than O(log n)" (line ~827).

- **Fallback.** `JdkBackedSetBuilderImpl` (line ~884) is documented as using "a JDK HashSet, which has built in hash flooding protection". Its `build()` returns `new JdkBackedImmutableSet<>(delegate, asImmutableList(dedupedElements, distinct))`.

- **Design note.** A comment near line 101 of the same file discusses whether hash flooding is a real concern. I only saw the grep hit for it and didn't read the surrounding text.

**Related, not checked in detail:** `ImmutableMap` has an analogous mechanism. `RegularImmutableMap.java:64` defines `HASH_FLOODING_FPP = 0.001`, and lines ~99-100 fall back to a `java.util.HashMap`-based implementation.

**Uncertainty:** I read the set code only, not the tests. I didn't confirm the false-positive claim that `ImmutableSetTest` checks.