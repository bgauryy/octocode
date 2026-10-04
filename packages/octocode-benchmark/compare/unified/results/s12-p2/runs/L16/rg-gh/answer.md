`maximumSize` and `concurrencyLevel` together decide how many segments the cache has and how much weight each segment may hold. Size-based eviction runs per segment, not across the whole cache. All line numbers below are at 4d41665af1, in `guava/src/com/google/common/cache/` (`LocalCache.java` unless noted).

**How the settings reach `LocalCache`**
- `concurrencyLevel(int)` stores the value in the builder (`CacheBuilder.java:463-469`). If you don't set it, the default is 4 (`CacheBuilder.java:199`, `:474`).
- `maximumSize(long)` stores `maximumSize` (`CacheBuilder.java:499-508`). `getMaximumWeight()` returns it when no weigher is set (`CacheBuilder.java:607`). Each entry then counts as weight 1.
- The `LocalCache` constructor caps the concurrency level at `MAX_SEGMENTS = 1 << 16` (`:150`, `:250`). It reads the maximum as `maxWeight` (`:255`).

**Segment count** (`:280-295`)
- Segments start at 1 and double while `segmentCount < concurrencyLevel`. When size-based eviction is on, doubling also requires `segmentCount * 20L <= maxWeight` (`:287-288`).
- The result is a power of two that is at least `concurrencyLevel`, unless the size cap stops it earlier. The cap keeps each segment at roughly 10 or more entries (the comment says 10, the `* 20` check is stricter).
- The code comment says the reason is that eviction happens per segment. Too many segments for a small maximum would make eviction look random (`:280-284`).
- A small `maximumSize` can therefore shrink the segment count below `concurrencyLevel`. For example, `maximumSize(100)` with `concurrencyLevel(16)` gives 4 segments, because 8 × 20 = 160 > 100.

**Segment lookup**
- The code sets `segmentShift = 32 - log2(segmentCount)` and `segmentMask = segmentCount - 1` (`:292-293`). The `segments` array is allocated at that size (`:295`).
- A key picks its segment with `segments[(hash >>> segmentShift) & segmentMask]` (`:1768`).

**Per-segment capacity**
- When size-based eviction is on, `maxWeight` is divided across the segments: `maxSegmentWeight = maxWeight / segmentCount + 1`. The first `maxWeight % segmentCount` segments keep that extra 1, and the rest get one less, so the per-segment limits sum exactly to `maxWeight` (`:307-317`).
- Each `Segment` stores its limit in `maxSegmentWeight` (`:1929`, `:1979`). When a segment's total weight exceeds that limit, it evicts entries (`:2672`).
- Without a maximum, segments are created with `UNSET_INT` as the limit (`:319-323`).
- The initial hash-table capacity is also split across segments. It is `initialCapacity / segmentCount`, rounded up, then rounded up again to a power of two (`:297-305`). With a non-custom weigher, `initialCapacity` is first capped at `maxWeight` (`:276-278`).

**Documented effect**
- The `CacheBuilder` Javadoc says that with `concurrencyLevel > 1`, each segment is limited to about `maximumSize / concurrencyLevel` (`CacheBuilder.java:481-483`). It also says `concurrencyLevel(1)` gives more deterministic eviction (`CacheBuilder.java:452`).

I read the main `guava/` copy only. The `android/` and GWT copies of `LocalCache.java` may differ, and I didn't check them.