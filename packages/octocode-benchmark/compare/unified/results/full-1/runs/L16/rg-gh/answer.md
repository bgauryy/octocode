**Answer:** `CacheBuilder` only stores these settings. `LocalCache`'s constructor uses them to decide how many `Segment`s the cache has, and how much weight budget each segment gets. Eviction by size then runs per segment, not across the whole cache.

All paths below are under `guava/src/com/google/common/cache/`.

**Builder side (`CacheBuilder.java`)**
- `concurrencyLevel` defaults to 4 (`DEFAULT_CONCURRENCY_LEVEL`, line 199). `getConcurrencyLevel()` returns that default when the field is unset (line 474).
- `maximumSize(long)` sets `maximumSize` (line 508). `getMaximumWeight()` returns `maximumSize` when no weigher is set (line 607). So `LocalCache` treats a size limit as a weight limit where every entry weighs 1.
- The Javadoc says that when `concurrencyLevel` is above 1, each segment gets its own limit of roughly `maximumSize / concurrencyLevel` (lines 481-483).

**Segment count (`LocalCache.java:250-295`)**
- The concurrency level is capped at `MAX_SEGMENTS = 1 << 16` (lines 150, 250).
- The segment count starts at 1 and doubles while `segmentCount < concurrencyLevel`. If size-based eviction is on, it also requires `segmentCount * 20L <= maxWeight` (lines 287-291). That gives a power of two, and each segment gets at least about 10 entries of budget. The comment at lines 280-284 explains why: eviction is per segment, so too many segments for a small maximum makes eviction look random.
- `segmentShift = 32 - log2(segmentCount)` and `segmentMask = segmentCount - 1` (lines 292-293). `segmentFor(hash)` picks a segment with `segments[(hash >>> segmentShift) & segmentMask]` (line 1768).
- `segments` is an array of `Segment` objects. Each is a separate hash table with its own lock and its own access queue (used for LRU-style eviction).
- Example: `maximumSize(100)` with `concurrencyLevel(16)` gives 4 segments, because 8 × 20 = 160 > 100, so doubling stops at 4.

**Per-segment maximum (`LocalCache.java:307-318`)**
- `maxSegmentWeight = maxWeight / segmentCount + 1`. It drops by 1 once the loop index reaches `maxWeight % segmentCount`. This makes the segment maximums add up exactly to `maxWeight`.
- When there is no size limit, segments get `UNSET_INT` instead (line 321).
- Each segment is created with its own initial table size. That size is the initial capacity divided by the segment count, rounded up to a power of two (lines 297-305).
- For a plain `maximumSize` with no custom weigher, the initial capacity is also capped at `maxWeight` (lines 276-278).

**Eviction (`Segment.evictEntries`, lines 2657-2676)**
- Each segment compares its own `totalWeight` to its own `maxSegmentWeight`. It evicts from the head of its own access queue until it is under the limit.
- If a single new entry is heavier than the segment maximum, that entry is removed by itself.
- So the cache as a whole can hold slightly fewer than `maximumSize` entries, and eviction is only approximately global LRU. `concurrencyLevel(1)` gives one segment and stricter eviction; the Javadoc mentions this at about line 452.

**Uncertainty:** I read the constructor and `evictEntries` closely. I did not trace the `Segment` constructor's table sizing beyond line 2000, nor the `UNSET_INT` branch of that constructor.