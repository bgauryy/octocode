**Short answer:** `concurrencyLevel` sets how many `Segment`s the `LocalCache` is split into. `maximumSize` sets the total weight budget. The budget is divided across those segments, and eviction runs per segment, not globally. `maximumSize` also caps how many segments get created. All paths below are under `guava/src/com/google/common/cache/`, and I read the files in the checkout without confirming it is at 4d41665af1 (git was unavailable).

**Builder side**
- `CacheBuilder.getConcurrencyLevel()` returns 4 when unset (`DEFAULT_CONCURRENCY_LEVEL = 4`, `CacheBuilder.java:199`, `:473-474`).
- `maximumSize(n)` only stores `this.maximumSize`. It rejects a second call, a prior `maximumWeight`, and a weigher (`CacheBuilder.java:499-509`).
- `getMaximumWeight()` returns `maximumSize` when there is no weigher, so each entry counts as weight 1. It returns 0 if `expireAfterWrite` or `expireAfterAccess` is 0 (`CacheBuilder.java:600-607`).

**`LocalCache` constructor**
- `concurrencyLevel = min(builder.getConcurrencyLevel(), MAX_SEGMENTS)`, with `MAX_SEGMENTS = 1 << 16` (`LocalCache.java:150`, `:250`).
- `maxWeight = builder.getMaximumWeight()` (`LocalCache.java:258`). `evictsBySize()` is true when `maxWeight >= 0` (`:326-327`).
- **Segment count:** it is the smallest power of two that is at least `concurrencyLevel`. With size-based eviction, the loop also stops growing once `segmentCount * 20 > maxWeight`, so each segment gets at least about 10 entries (`LocalCache.java:280-291`).
  - The code comment says eviction is per segment, so too many segments relative to the maximum size would give random eviction behavior.
  - The comment says 10 entries per segment, but the check `segmentCount * 20L <= maxWeight` allows roughly 10 or more. I did not work out the exact bound.
- **Segment lookup:** `segmentShift = 32 - log2(segmentCount)` and `segmentMask = segmentCount - 1`. A key's segment is `segments[(hash >>> segmentShift) & segmentMask]` (`LocalCache.java:292-293`, `:1766-1769`).
- **Initial table size:** `initialCapacity` is first capped at `maxWeight` when eviction is by size with no custom weigher (`LocalCache.java:276-277`). It is then divided across segments, rounded up, and rounded up again to a power of two for each segment's table (`:297-305`).
- **Per-segment budget:** `maxSegmentWeight = maxWeight / segmentCount + 1`, decremented once `i == maxWeight % segmentCount`. This makes the segment limits sum exactly to `maxWeight`. Each segment is built with `createSegment(segmentSize, maxSegmentWeight, stats)` (`LocalCache.java:307-317`). Without size-based eviction, the limit is `UNSET_INT` (`:319-322`).

**Eviction**
- `Segment.evictEntries` returns early unless `evictsBySize()` (`LocalCache.java:2658`).
- It removes the newest entry if that entry alone outweighs `maxSegmentWeight` (`:2666`).
- It then evicts the least-recently-used entries (`getNextEvictable()`) while `totalWeight > maxSegmentWeight` (`:2672-2676`).
- The cap is therefore enforced per segment. The cache as a whole can start evicting before it reaches `maximumSize` if entries hash unevenly across segments.

**Not verified:** I did not check git history, and I did not check how the `Segment` constructor sizes its table beyond the `maxSegmentWeight` handling.