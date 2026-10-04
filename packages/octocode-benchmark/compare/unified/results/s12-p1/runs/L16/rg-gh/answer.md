**Short answer:** `concurrencyLevel` sets how many `Segment`s the `LocalCache` has (a power of two). `maximumSize` is stored as `maxWeight`. It can cap that segment count, and it is divided across the segments as a per-segment limit. Paths below are under `guava/src/com/google/common/cache/`. I did not run anything. The line numbers come from reading `LocalCache.java` and `CacheBuilder.java` at the pinned commit.

**Builder side**
- `concurrencyLevel` defaults to 4 (`CacheBuilder.java:199`, `:474`).
- `maximumSize` is stored in the `maximumSize` field (`CacheBuilder.java:499-508`). `getMaximumWeight()` returns it when no weigher is set (`CacheBuilder.java:607`).
- The Javadoc says that with `concurrencyLevel > 1`, each segment is limited to roughly `maximumSize / concurrencyLevel` (`CacheBuilder.java:481-483`).

**`LocalCache` constructor**
- **Segment count:** `concurrencyLevel` is first capped at `MAX_SEGMENTS = 1 << 16` (`LocalCache.java:150`, `:250`). `maxWeight = builder.getMaximumWeight()` is read at `:258`.
- The segment count starts at 1 and doubles while `segmentCount < concurrencyLevel`. When the cache evicts by size, it also requires `segmentCount * 20 <= maxWeight` (`:285-291`). So a small `maximumSize` reduces the number of segments, because each segment keeps at least about 20 weight units.
- **Hash routing:** `segmentShift = 32 - log2(segmentCount)` and `segmentMask = segmentCount - 1` (`:292-293`). `segmentFor(hash)` picks `segments[(hash >>> segmentShift) & segmentMask]` (`:1766-1768`).
- **Per-segment limit:** `maxSegmentWeight = maxWeight / segmentCount + 1`. It is decremented once index `i` reaches `maxWeight % segmentCount`, so the segment limits sum exactly to `maxWeight` (`:307-317`). Without size eviction, `UNSET_INT` is passed instead (`:319-322`).
- **Initial table size:** `initialCapacity` is first capped at `maxWeight` when size-based eviction is on without a custom weigher (`:275-278`). It is then divided across the segments and rounded up to a power of two to get each segment's table size (`:297-304`).
- **Segment constructor:** it stores `maxSegmentWeight` (`:1979`) and builds its own entry table, using the capacity passed in (`:1981`).
- **Eviction scope:** eviction is decided per segment against `maxSegmentWeight`. An entry heavier than `maxSegmentWeight` is evicted immediately (`:2666`). The cache therefore does not enforce a strict global LRU. This is why the Javadoc suggests `concurrencyLevel(1)` for more deterministic eviction (`CacheBuilder.java:452`).

**Uncertainty:** I did not read the eviction code beyond `:2666`, so I have not described the LRU mechanics inside a segment.