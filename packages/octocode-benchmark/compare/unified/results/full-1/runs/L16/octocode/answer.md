The two settings decide how many segments `LocalCache` creates, how large each segment's hash table is, and how the size cap is divided across segments. The evidence is from `guava/src/com/google/common/cache/`. I did not read the bodies of `CacheBuilder.maximumSize` and `concurrencyLevel`, only their declaration lines.

**Builder side**
- `CacheBuilder.java:199` sets `DEFAULT_CONCURRENCY_LEVEL = 4`.
- `concurrencyLevel(int)` is at `CacheBuilder.java:463` and `getConcurrencyLevel()` at `:473`.
- `maximumSize(long)` is at `CacheBuilder.java:499`.
- These are only settings. The structure is built in the `LocalCache` constructor.

**Segment count from `concurrencyLevel`** (`LocalCache.java:250`, `:285-293`)
- The level is first capped at `MAX_SEGMENTS`: `concurrencyLevel = min(builder.getConcurrencyLevel(), MAX_SEGMENTS)`.
- The segment count is a power of two. The loop starts at `segmentCount = 1` and doubles it while `segmentCount < concurrencyLevel`. The result is the smallest power of two that reaches the level, so the count is the level rounded up to a power of two. The code comment says "exceeds", but the `<` test also stops when the count equals the level.
- From that count it derives `segmentShift = 32 - shift` and `segmentMask = segmentCount - 1`. These select a segment from a key's hash bits.
- It then allocates `this.segments = newSegmentArray(segmentCount)`.

**Effect of `maximumSize`** (`LocalCache.java:287-289`, `:295-311`)
- `maximumSize` becomes `maxWeight`, and `evictsBySize()` becomes true.
- The doubling loop then has an extra condition, `segmentCount * 20L <= maxWeight`. So a small maximum size means fewer segments than the concurrency level asks for.
- The code comment says the goal is at least 10 entries per segment, but the check is `* 20L`. Because the count doubles, this yields at least 10 per segment after the last doubling.
- The reason given is that size eviction happens per segment, not globally. Too many segments would make eviction look random.
- Each segment gets its own cap. `maxSegmentWeight = maxWeight / segmentCount + 1`, and the `maxWeight % segmentCount` remainder is used to decrement it from segment index `remainder` onward. The segment caps therefore sum exactly to `maxWeight`.
- Without size eviction, segments are created with `UNSET_INT` as their cap.
- For unweighted caches, the initial capacity is also clamped to `maxWeight` (`:280-282`).

**Per-segment table sizing** (`LocalCache.java:2000` and the constructor code above)
- Each segment's initial capacity is `initialCapacity / segmentCount`, rounded up. That is then rounded up to a power of two for `segmentSize`.
- Each `Segment` gets its own `AtomicReferenceArray` table and its own `maxSegmentWeight`, stats counter, and access/write/recency queues.
- The segment constructor calls `initTable`. That sets `threshold = length * 3 / 4`. When there is no custom weigher and the threshold equals `maxSegmentWeight`, it adds 1 to avoid expanding the table before eviction (`LocalCache.java:1976-1979`).

**Net effect:** `concurrencyLevel` sets an upper bound on the number of independently locked segments (rounded to a power of two). `maximumSize` can reduce that number and splits the size cap across the segments.

**Uncertainty:** I did not look up the value of `MAX_SEGMENTS`. I only read the `LocalCache` and `Segment` code paths listed above.