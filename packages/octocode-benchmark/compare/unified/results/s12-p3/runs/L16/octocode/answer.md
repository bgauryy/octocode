`maximumSize` and `concurrencyLevel` together decide how many segments the cache has and how much capacity each segment gets. Everything below is from `LocalCache`'s constructor at `guava/src/com/google/common/cache/LocalCache.java`. I did not open the `CacheBuilder` setter bodies (`maximumSize` at line 499 of `CacheBuilder.java`, `maximumWeight` at line 542), so how `maximumSize` is stored as `maxWeight` comes from the `LocalCache` side only.

**Segment count (`concurrencyLevel`)**
- The constructor caps the level with `min(builder.getConcurrencyLevel(), MAX_SEGMENTS)` (line 250). The `CacheBuilder` default is 4 (`CacheBuilder.java:199`).
- It then grows `segmentCount` by doubling from 1, counting each doubling in `segmentShift`. The loop is at lines 285–291 and stops once `segmentCount >= concurrencyLevel`. So the segment count is the smallest power of two that is at least the concurrency level, and the comment at line 280 describes this as "exceeds".
- It stores `segmentShift = 32 - segmentShift` and `segmentMask = segmentCount - 1` (lines 292–293), then allocates `segments = newSegmentArray(segmentCount)` (line 295).
- A key is routed to its segment by `segments[(hash >>> segmentShift) & segmentMask]` (line 1768).

**How `maximumSize` limits the segment count**
- The doubling loop has an extra condition: `(!evictsBySize() || segmentCount * 20L <= maxWeight)` (lines 287–288). With size-based eviction, segments stop doubling once there would be fewer than 20 entries of max weight per segment.
- The comment at lines 280–284 says each segment should get at least 10 entries. The code's `* 20L` check gives a minimum of about 20.
- The stated reason is that eviction happens per segment, not globally. Too many segments for a small maximum size would make eviction look random.
- So a small `maximumSize` can reduce the effective concurrency below what you asked for.

**Per-segment capacity (`maximumSize`)**
- When `evictsBySize()` is true, each segment gets its own limit, `maxSegmentWeight`. The base value is `maxWeight / segmentCount + 1`. It drops by 1 once the segment index reaches `maxWeight % segmentCount`, so the segment limits sum exactly to `maxWeight` (lines 307–317).
- Without size-based eviction, segments are created with `UNSET_INT` (lines 318–322).
- Each segment enforces its own limit. In `evictEntries`, it evicts the newest entry if that entry alone exceeds `maxSegmentWeight` (line 2666). Otherwise it evicts from the segment's access queue while `totalWeight > maxSegmentWeight` (lines 2672–2677).

**Initial table size**
- If there is no custom weigher, `initialCapacity` is capped at `maxWeight` (lines 276–278).
- The per-segment capacity is `initialCapacity / segmentCount`, rounded up (lines 297–300). The segment's table size is that value rounded up to a power of two (lines 302–305).
- The segment resize logic bumps `threshold` by one when `threshold == maxSegmentWeight` and there is no custom weigher, "to prevent spurious expansion before eviction" (lines 2000–2002).

**Uncertainty:** I did not look up the value of `MAX_SEGMENTS`.