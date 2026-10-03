**Answer:** `CacheBuilder` only stores the settings. `LocalCache`'s constructor turns them into a fixed array of `Segment`s, and `concurrencyLevel` and `maximumSize` together decide how many segments there are and how much weight each one gets. All line numbers are in `java/guava/src/com/google/common/cache/LocalCache.java` at the pinned commit unless noted.

**concurrencyLevel → number of segments**
- If you don't set it, `CacheBuilder.getConcurrencyLevel()` returns the default of 4 (`CacheBuilder.java:199`, `:474`).
- `LocalCache` starts with `segmentCount = 1` and doubles it while `segmentCount < concurrencyLevel` (lines 285-291). The result is the smallest power of two that reaches `concurrencyLevel`.
- It then sets `segmentShift = 32 - segmentShift` and `segmentMask = segmentCount - 1` (lines 292-293). These are used to pick a segment from a key's hash.
- It allocates `segments = newSegmentArray(segmentCount)` (line 295).
- Each segment's initial table size is `initialCapacity / segmentCount`, rounded up (lines 297-300) and then up to a power of two (lines 302-305).

**maximumSize → cap on segment count and per-segment weight**
- When size-based eviction is on, the doubling loop has an extra condition: `segmentCount * 20L <= maxWeight` (lines 287-288). This caps the segment count so each segment gets roughly 10 or more entries. The code comment (lines 280-284) says eviction happens per segment rather than globally, so too many segments would make eviction look random.
- If `evictsBySize()` is true and there is no custom weigher, `initialCapacity` is also capped at `maxWeight` (lines 274-276).
- Each segment gets its own `maxSegmentWeight = maxWeight / segmentCount + 1`. The first `remainder = maxWeight % segmentCount` segments keep the `+1` and the rest drop it, so the segment maxima sum exactly to `maxWeight` (lines 308-316).
- Without size eviction, segments are created with `UNSET_INT` as their max weight (lines 318-322).

**Consequence:** `maximumSize` is enforced per segment, not as one global limit. A small `maximumSize` also reduces the effective concurrency below the requested `concurrencyLevel`.

**Uncertainty:** I didn't read `evictsBySize()` or `maxWeight` (declared around line 179, not read). I also didn't read `createSegment`, or how the segment is chosen from the hash.