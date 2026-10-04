**Short answer:** `concurrencyLevel` sets how many `Segment`s the cache has. `maximumSize` caps that count and is divided across the segments. Eviction then runs inside each segment, not across the whole cache. Everything below is from `LocalCache.java` and `CacheBuilder.java` at the pinned commit.

**How `concurrencyLevel` is set**
- The builder default is 4 (`CacheBuilder.java:199`, applied in `getConcurrencyLevel()` at `CacheBuilder.java:473-474`).
- The `LocalCache` constructor clamps it to `MAX_SEGMENTS`, which is 1<<16 (`LocalCache.java:150`, `LocalCache.java:250`).

**Segment count** (`LocalCache.java:285-295`)
- The segment count is the smallest power of two that reaches `concurrencyLevel`. The loop starts at `segmentCount = 1` and doubles it.
- If the cache evicts by size, the loop also stops once `segmentCount * 20 > maxWeight`. The comment at lines 280-284 says this keeps each segment at roughly 10 or more entries. Size-based eviction is per segment, so too many segments against a small maximum would give random-looking eviction.
- `segmentMask = segmentCount - 1`, and `segmentShift = 32 - log2(segmentCount)`.
- `segments` is an array of `Segment` objects, each a `ReentrantLock` subclass that is its own hash table (`LocalCache.java:1867`, `LocalCache.java:188`). Its entry table is an `AtomicReferenceArray` (`LocalCache.java:1926`).
- Keys are routed with `segments[(hash >>> segmentShift) & segmentMask]` (`LocalCache.java:1768`). This uses the upper bits of the hash.

**`maximumSize`**
- `LocalCache` stores it as `maxWeight` (`LocalCache.java:258`, from `builder.getMaximumWeight()`). `evictsBySize()` is `maxWeight >= 0` (`LocalCache.java:326-328`).
- `maximumSize` is divided across the segments (`LocalCache.java:307-317`). Each segment gets `maxWeight / segmentCount`, and the first `remainder` segments get one extra. The segment limits therefore add up exactly to `maxWeight`. Each limit is stored as that segment's `maxSegmentWeight` (`LocalCache.java:1929`).
- Each segment evicts independently. `evictEntries` runs while `totalWeight > maxSegmentWeight` and removes entries with `RemovalCause.SIZE` (`LocalCache.java:2658-2675`). An entry heavier than the whole segment limit is removed on its own (`LocalCache.java:2666`).
- When there is no custom weigher, the initial capacity is capped at `maxWeight` (`LocalCache.java:276-278`). The per-segment initial capacity is `initialCapacity / segmentCount`, rounded up (`LocalCache.java:297-300`).
- `initTable` nudges the resize threshold up by one if it would equal `maxSegmentWeight`, so the table does not expand before eviction starts (`LocalCache.java:2000-2003`).
- Size-based eviction also turns on the access queue (`usesAccessQueue()`, `LocalCache.java:350-352`).

**Uncertainty:** I didn't read lines 301-306. They appear to compute `segmentSize` as a power of two at least `segmentCapacity`, but I didn't confirm that. I also didn't read `CacheBuilder.maximumSize` or `getMaximumWeight` in full. That means I didn't check how `maximumSize` maps to weight, or the 0 return at `CacheBuilder.java:604-606`.