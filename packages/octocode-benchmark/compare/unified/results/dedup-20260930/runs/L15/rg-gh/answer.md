**Short answer:** `ImmutableSet` uses an open-addressed, linear-probing hash table. While building, it watches for abnormally long runs of filled slots. If it sees one, it abandons its own table and falls back to a `java.util.HashSet`-backed implementation. The JDK set has its own flooding protection. All paths below are under `guava/src/com/google/common/collect/`, at commit 4d41665.

**1. Online detection while adding** (`RegularSetBuilderImpl`, `ImmutableSet.java:663-727`)
- The class doc says it "attempts to detect hash flooding, and if it's identified, falls back to JdkBackedSetBuilderImpl" (`:663-666`).
- `insertInHashTable` (`:706-727`) probes linearly from `Hashing.smear(hash)`. The loop is `for (i = i0; i - i0 < maxRunBeforeFallback; i++)` (`:711`).
- If the probe run reaches `maxRunBeforeFallback` without finding a null slot or an equal element, it returns `new JdkBackedSetBuilderImpl<E>(this).add(e)` (`:725`).
- This bounds construction cost when many elements share the same hash code, which would otherwise be O(n²).

**2. Threshold** (`ImmutableSet.java:813-820, 878-880`)
- `maxRunBeforeFallback(tableSize) = MAX_RUN_MULTIPLIER * log2(tableSize)`, with `MAX_RUN_MULTIPLIER = 13`.
- The comment says the value was "determined experimentally" to match the desired false-positive probability (`:808-814`).

**3. Whole-table check at build time** (`hashFloodingDetected`, `ImmutableSet.java:835-870`, called from `review()` at `:744`)
- The doc (`:822-834`) explains why this second check is needed. The online check catches exactly matching hash codes. It can't catch hash codes adversarially designed to land in ascending table slots. That pattern keeps construction O(n) but makes later queries O(n).
- This method scans the whole table for any run longer than the threshold, skipping ahead where possible. The doc says it takes "O(n) in the worst case, O(n / log n) on average".
- If it returns false, no query takes more than O(log n) (`:831`).
- If it returns true, `review()` returns a `JdkBackedSetBuilderImpl` (`:744`).

**4. The fallback** (`JdkBackedSetBuilderImpl`, `ImmutableSet.java:883-912`)
- It copies the deduplicated elements into a JDK `HashSet` (`newHashSetWithExpectedSize`, `:889-897`). Its doc says the JDK set "has built in hash flooding protection" (`:884-885`).

**Related classes**
- `RegularImmutableMap` caps the key-bucket chain at `MAX_HASH_BUCKET_LENGTH = 8` (`:70`). It throws `BucketOverflowException` when a chain grows longer (`:250-251`).
- `RegularImmutableMultiset` uses `MAX_HASH_BUCKET_LENGTH = 9` (`:119`). Its `hashFloodingDetected` (`:89-99`) makes the multiset fall back to `JdkBackedImmutableMultiset` (`:83-84`).

**Uncertainty**
- I read the `BucketOverflowException` throw sites in `RegularImmutableMap` but not the code that catches it. I didn't check how the map handles the fallback, so I'm not claiming what it falls back to.
- I didn't open the `ImmutableSet.java` code for `build()` beyond the `case 0`/`case 1` branches. I also didn't look at the tests.