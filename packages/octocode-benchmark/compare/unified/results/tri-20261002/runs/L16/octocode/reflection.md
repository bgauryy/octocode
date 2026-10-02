**1. Helped:**
- The first parallel pair of `localSearch` calls did most of the work. One regex on `LocalCache.java` (`segmentShift|segmentMask|maxSegmentWeight|concurrencyLevel`) found the constructor and the `segmentFor` lookup at `:1768`. The other, on `CacheBuilder.java`, found the builder setters and the default of 4.
- The `localFetch` of `LocalCache.java` lines 248-325 and 1972-2005 showed the segment-count loop, the weight split and the `Segment` constructor in full, with the explanatory comment.

**2. Did not help:**
- My first two `localFetch` calls failed schema validation because I passed `ranges` as nested arrays instead of strings like `"248-325"`. That cost one wasted round trip.
- The `CacheBuilder.java` search was noisy. It returned about 60 matches, many of them Javadoc.
- I never fetched the `MAX_SEGMENTS` definition. I also cited `:2666` and `:2672` from grep match lines alone, without reading their context, so I only know what those two lines say.

**3. Next time:** I would use the string range format from the start and add a narrower search for `MAX_SEGMENTS`. I would also fetch the eviction code around `:2660-2680`. I would use `matchString` or a tighter regex on `CacheBuilder.java` to avoid the Javadoc noise.

**4. Confidence:** High on the segment-count, per-segment weight and table-sizing claims, since I read those lines directly. Medium on the eviction-loop claim and the `MAX_SEGMENTS` cap, which I only partly verified, and the answer said so for `MAX_SEGMENTS`.