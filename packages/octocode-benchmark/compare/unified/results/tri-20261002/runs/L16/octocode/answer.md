`maximumSize` and `concurrencyLevel` together decide how many segments the cache has and how much weight each segment may hold. All of this is in `LocalCache`, which `CacheBuilder` builds. Everything below is from `guava/src/com/google/common/cache/`.

**Builder side (`CacheBuilder.java`)**
- `concurrencyLevel(int)` must be positive (`:468`). If unset, `getConcurrencyLevel()` returns `DEFAULT_CONCURRENCY_LEVEL`, which is 4 (`:199`, `:473-474`).
- `maximumSize(long)` must be non-negative and cannot be combined with `maximumWeight` (`:499-508`). The builder's `getMaximumWeight()` returns `maximumSize` when no weigher is set (`:607`).
- The Javadoc says that when `concurrencyLevel > 1`, each segment's limit is about `maximumSize / concurrencyLevel` (`:481-483`).

**Segment count (`LocalCache.java:250`, `:285-295`)**
- `concurrencyLevel` is capped at `MAX_SEGMENTS` (`:250`).
- `segmentCount` starts at 1 and doubles while it is below `concurrencyLevel`. When the cache evicts by size, doubling also requires `segmentCount * 20L <= maxWeight` (`:287-291`).
- The comment at `:280-284` explains why: size-based eviction runs per segment, so too many segments for a small maximum would make eviction effectively random.
- `segmentCount` is therefore a power of two. A small `maximumSize` can leave it below the requested `concurrencyLevel`.
- `segmentShift = 32 - log2(segmentCount)` and `segmentMask = segmentCount - 1` (`:292-293`). A key's segment is `segments[(hash >>> segmentShift) & segmentMask]` (`:1768`).

**Per-segment size limit (`:307-317`)**
- When the cache evicts by size, each segment gets `maxSegmentWeight = maxWeight / segmentCount + 1`. The `+1` is dropped by one (`maxSegmentWeight--`) once the loop index `i` reaches `maxWeight % segmentCount` (`:312-313`). This keeps the segment limits summing exactly to `maxWeight` (comment at `:308`).
- Without size-based eviction, segments are created with `UNSET_INT` as the limit (`:321`).
- Eviction is enforced per segment, not globally. Each segment evicts while `totalWeight > maxSegmentWeight` (`:2672`). An entry heavier than `maxSegmentWeight` is also evicted (`:2666`).

**Per-segment table and queues**
- Without a custom weigher, `initialCapacity` is capped at `maxWeight` (`:276-277`).
- Each segment's table capacity is `initialCapacity / segmentCount`, rounded up, then rounded up again to a power of two (`:297-305`).
- Each `Segment` has its own `AtomicReferenceArray` table, along with its own write queue, access queue and recency queue (`:1981-1991`).
- The table's resize threshold is 0.75 of its length. If it equals `maxSegmentWeight` and there is no custom weigher, it is bumped by 1 so the table does not expand before eviction starts (`:1998-2003`).

I did not read the `MAX_SEGMENTS` definition, so I don't have its value.