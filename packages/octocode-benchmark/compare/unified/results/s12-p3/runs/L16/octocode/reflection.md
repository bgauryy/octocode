**1. Helped:**
- The first `localSearch` on `LocalCache.java` (regex `segmentShift|segmentMask|maxSegmentWeight|segmentCount|concurrencyLevel`) located every relevant line in one call.
- One `localFetch` with three ranges then read the constructor (245–325) and the eviction loop (2660–2685) together. That was enough to answer the segment-count and per-segment-limit parts.

**2. Did not help:**
- The `localSearch` on `CacheBuilder.java` returned only grep lines, mostly Javadoc mentions, and I never read the setter bodies. Its read hint was not followed.
- Several claims rest on search output rather than a read in context, such as the line 1768 routing and the line 2000 threshold bump. I never fetched `evictsBySize()`, `customWeigher()` or `MAX_SEGMENTS`.
- The line 2672 loop is cited from a fetch that began mid-method, so I did not see the start of `evictEntries`.

**3. Next time:**
- Add a `matchString` fetch for `maximumSize(long` in `CacheBuilder.java` and one for `MAX_SEGMENTS`.
- Fetch `evictsBySize` in the same batch as the constructor.
- Batch these reads in parallel in one round.

**4. Confidence:** medium-high. The segment-count formula, the `* 20L` guard and the per-segment weight split were all seen directly. The gaps are how `maximumSize` maps to `maxWeight` and the `MAX_SEGMENTS` value, and the answer says both are unverified.