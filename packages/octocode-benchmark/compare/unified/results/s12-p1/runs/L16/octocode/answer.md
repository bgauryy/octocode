`concurrencyLevel` sets how many segments the cache is split into. `maximumSize` sets each segment's weight budget, and it also caps the segment count. Eviction happens per segment, not across the whole cache. All lines below are in `guava/src/com/google/common/cache/LocalCache.java`, except where I name `CacheBuilder.java`.

**How the builder values flow in**
- `concurrencyLevel` defaults to 4 (`CacheBuilder.java:199`, `:474`). The `LocalCache` constructor clamps it to `MAX_SEGMENTS` (`:250`).
- `maximumSize(n)` only stores `maximumSize` (`CacheBuilder.java:508`). `getMaximumWeight()` returns `maximumSize` when there is no weigher (`CacheBuilder.java:607`). `LocalCache` reads that as `maxWeight` (`:258`), so size is treated as a weight where each entry weighs 1.

**Segment count (`:285-293`)**
- `segmentCount` starts at 1 and doubles while `segmentCount < concurrencyLevel`.
- If the cache evicts by size, it also requires `segmentCount * 20L <= maxWeight`. This keeps roughly 10 or more entries per segment. The comment at `:280-284` says too many segments relative to the maximum size would make eviction effectively random.
- So the result is a power of two, at most `concurrencyLevel` rounded up. A small `maximumSize` reduces it.
- `segmentShift = 32 - log2(segmentCount)` and `segmentMask = segmentCount - 1`. `segments` is allocated at `:295`.
- A key's segment is `segments[(hash >>> segmentShift) & segmentMask]` (`:1768`).

**Per-segment table size (`:297-305`)**
- `initialCapacity` is first capped at `maxWeight` when there is no custom weigher (`:276-277`).
- Each segment gets `initialCapacity / segmentCount`, rounded up, then rounded up again to a power of two (`segmentSize`).
- `Segment.initTable` sets the resize threshold to 0.75 × table length (`:1999`). It bumps the threshold by one if it equals `maxSegmentWeight` and there is no custom weigher, to avoid resizing just before eviction (`:2000-2002`).

**Per-segment size limit (`:307-317`)**
- When the cache evicts by size, `maxSegmentWeight = maxWeight / segmentCount + 1`. It is decremented once `i == maxWeight % segmentCount`, so the segment limits sum exactly to `maxWeight`.
- Without size-based eviction, segments get `UNSET_INT` (`:319-322`).
- Each `Segment` enforces its own limit: it evicts while `totalWeight > maxSegmentWeight` (`:2672`). An entry heavier than the whole segment limit is removed on its own (`:2666`).

**Practical effect**
- The cache-wide maximum is a sum of per-segment limits, so eviction is approximate. An entry can be evicted while other segments still have room.
- The `CacheBuilder` Javadoc says the same (`CacheBuilder.java:452`, `:481-483`). It notes that `concurrencyLevel(1)` is a way to get more deterministic eviction.

I did not read the code between `:323` and `:1994`.