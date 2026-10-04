`maximumSize` and `concurrencyLevel` together decide how many segments `LocalCache` has and how much capacity each segment gets. All line numbers below are in `guava/src/com/google/common/cache/` at the pinned commit. Some shell calls printed `/dev/null: Operation not permitted`, but the file output was intact.

**1. Builder inputs**
- `concurrencyLevel` defaults to 4 (`CacheBuilder.java:199`, `:473-474`). It must be positive (`:468`).
- `maximumSize(n)` stores `maximumSize` (`CacheBuilder.java:508`). With no weigher, `getMaximumWeight()` returns that value as the max weight (`:603-607`).
- The Javadoc says that when `concurrencyLevel` is greater than 1, each segment's limit is roughly `maximumSize / concurrencyLevel` (`:481-483`).

**2. Segment count (`LocalCache.java:250-295`)**
- `concurrencyLevel` is capped at `MAX_SEGMENTS = 1 << 16` (`:150`, `:250`).
- The cache doubles `segmentCount` from 1 while `segmentCount < concurrencyLevel`. If the cache evicts by size, it also requires `segmentCount * 20 <= maxWeight` (`:285-291`).
- So the segment count is a power of two that reaches at least `concurrencyLevel`, and a small `maximumSize` reduces it. Each segment keeps at least about 20 units of weight. For example, `maximumSize(10)` gives a single segment.
- `segmentShift = 32 - log2(segmentCount)` and `segmentMask = segmentCount - 1` (`:292-293`). A key's segment is `segments[(hash >>> segmentShift) & segmentMask]` (`:1768`).
- The `segments` array is allocated with `segmentCount` entries (`:295`).

**3. Per-segment sizing**
- The initial capacity is split across segments and rounded up to a power of two, `segmentSize` (`:297-305`).
- When `evictsBySize()` is true (`maxWeight >= 0`, `:326-327`), each segment gets its own `maxSegmentWeight` (`:307-317`). It starts at `maxWeight / segmentCount + 1`, and the `+1` is dropped for segments with index `>= maxWeight % segmentCount`. The per-segment limits therefore sum exactly to `maxWeight`.
- Without a size limit, segments get `UNSET_INT` (`:320-321`).
- If there is no custom weigher, the initial capacity is also capped at `maxWeight` (`:276-277`).

**4. Effect on eviction**
- Eviction is per segment, not global. `Segment.evictEntries` evicts while the segment's `totalWeight > maxSegmentWeight` (`:2672`). An entry heavier than `maxSegmentWeight` is evicted right away (`:2666`).
- As a result, the whole cache can hold fewer entries than `maximumSize` when keys are unevenly spread across segments. `concurrencyLevel(1)` gives the most deterministic eviction, which the `CacheBuilder` Javadoc also notes (`CacheBuilder.java:452`).

I didn't read the `Segment.evictEntries` body beyond lines 2658-2672.