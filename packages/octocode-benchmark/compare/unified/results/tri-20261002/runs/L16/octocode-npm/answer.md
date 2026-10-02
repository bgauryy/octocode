Together, `maximumSize` and `concurrencyLevel` decide how many segments the cache has and how much capacity and weight each segment gets. All line numbers are in `guava/src/com/google/common/cache/`. `android/guava/` has a parallel copy of both files with the same logic at slightly shifted lines.

**Segmented structure.** `LocalCache` is an array of `Segment`s, and each segment is its own hash table (`LocalCache.java:189`, `:191`). A key picks its segment with `segments[(hash >>> segmentShift) & segmentMask]` (`LocalCache.java:1767`).

**`concurrencyLevel`**
- `CacheBuilder` stores the value and defaults it to 4 (`CacheBuilder.java:199`, `:474`). `LocalCache` caps it at `MAX_SEGMENTS`, which is 1<<16 (`LocalCache.java:250`, `:153`).
- The segment count is the smallest power of two that is at least `concurrencyLevel` (`LocalCache.java:285-291`).
- `segmentShift` ends up as `32 - log2(segmentCount)`, and `segmentMask` as `segmentCount - 1` (`LocalCache.java:292-293`). The segment array is then allocated at that size (`:295`).
- The `CacheBuilder` javadoc (about `:452`) says access and write queues are kept per segment. It also says tests often use `concurrencyLevel(1)` to get deterministic eviction.

**`maximumSize`**
- `maximumSize(n)` sets `CacheBuilder.maximumSize`, and `getMaximumWeight()` returns it as the weight when no weigher is set (`CacheBuilder.java:508-513`, `:603-608`). `LocalCache` reads it as `maxWeight`, and `evictsBySize()` is `maxWeight >= 0` (`LocalCache.java:258`, `:326`).
- It caps the segment count. When size-based eviction is on, the loop only doubles the segment count while `segmentCount * 20L <= maxWeight`. That gives each segment at least about 20 entries of budget, whereas the code comment says "at least 10" (`LocalCache.java:280-288`). The comment's reason is that eviction happens per segment, so too many segments for a small maximum would evict at seemingly random times.
- It is split across segments. `maxSegmentWeight = maxWeight / segmentCount + 1`, and the "+1" is dropped for segments from index `remainder` onward. The per-segment limits therefore sum to exactly `maxWeight` (`LocalCache.java:307-317`). The `CacheBuilder` javadoc (about `:481`) says each segment limits its own size independently, so an entry can be evicted before the global limit is reached.
- With the default one-per-entry weigher, `maximumSize` also clamps the initial capacity to `maxWeight` (`LocalCache.java:276-277`).

**Initial capacity.** The initial capacity is divided across segments, rounded up, and then rounded up again to a power of two for each segment's table size (`LocalCache.java:297-305`). Without a size limit, segments get `UNSET_INT` as their max weight (`:321`).

I did not read the `Segment` class itself, so how it enforces `maxSegmentWeight` is not covered here.