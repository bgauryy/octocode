**Short answer:** `concurrencyLevel` sets how many `Segment`s the cache has. `maximumSize` is split across those segments as a per-segment weight limit. `maximumSize` can also reduce the segment count. All of this happens in the `LocalCache` constructor, in `guava/src/com/google/common/cache/LocalCache.java`.

**concurrencyLevel → number of segments**
- `CacheBuilder.getConcurrencyLevel()` returns 4 if you didn't set it (`CacheBuilder.java:199`, `:473-474`).
- `LocalCache` caps it at `MAX_SEGMENTS` (`:250`).
- The segment count is the smallest power of two that is at least `concurrencyLevel` (`:285-290`). The loop is `while (segmentCount < concurrencyLevel && (!evictsBySize() || segmentCount * 20L <= maxWeight))`.
- Two fields route keys to segments: `segmentShift = 32 - log2(count)` and `segmentMask = count - 1` (`:292-293`).
- `segmentFor(hash)` picks the segment with `segments[(hash >>> segmentShift) & segmentMask]` (`:1768`).
- The `segments` array is allocated at that size (`:295`).

**maximumSize → per-segment limits**
- `maximumSize` becomes `maxWeight`. Without a custom weigher each entry has weight 1, so it is an entry count.
- **Fewer segments for small caches.** When the cache evicts by size, the loop only adds segments while `segmentCount * 20 <= maxWeight` (`:288`). Each segment therefore gets at least about 20 entries of capacity. A small `maximumSize` can produce fewer segments than `concurrencyLevel` asked for.
- **Split across segments.** `maxSegmentWeight = maxWeight / segmentCount + 1`, and it drops by 1 once the segment index reaches `maxWeight % segmentCount` (`:309-314`). The per-segment maxima add up to exactly `maxWeight`.
- **Enforced per segment.** Each segment gets its own `maxSegmentWeight` (`:316`, field at `:1929`). Eviction runs inside a segment when its `totalWeight` exceeds that limit (`:2672`). The limit is therefore not global, and LRU order is tracked per segment.
- **Initial capacity.** Without a custom weigher, `initialCapacity` is also capped at `maxWeight` (`:276-277`).
- **Table sizing.** Each segment's initial table size is `initialCapacity / segmentCount`, rounded up, then rounded up to a power of two (`:297-305`). `initTable` sets the resize threshold to 3/4 of the table length (`:1996`). If that threshold equals `maxSegmentWeight` and there is no custom weigher, it is bumped by 1 to avoid resizing just before eviction would start (`:2000-2001`).
- **No size limit.** Every segment gets `UNSET_INT` as its max weight (`:319-321`).

**Uncertainty:** I did not read `MAX_SEGMENTS`'s value or the `CacheBuilder.maximumSize` and `concurrencyLevel` setter validation. I also did not check how `maxWeight` is computed from `maximumSize` beyond the constructor's use of it.