1. **Helped:** The second Bash call was the one that got the answer. It ran `grep -nE` over `LocalCache.java` for `segmentShift|segmentMask|maxWeight|evictsBySize|concurrencyLevel|MAX_SEGMENTS`, plus a grep of `CacheBuilder.java`. That call surfaced the constructor logic at lines 250-322. The follow-up `sed -n 296,322p` and `sed -n 1972,1990p` confirmed the per-segment weight split and the `Segment` constructor.

2. **Did not help:**
   - `rg` was not installed, so my first call wasted a round trip. I switched to `grep`.
   - Every Bash call printed `/dev/null: Operation not permitted`. This was harmless noise.
   - I never confirmed the checkout was at 4d41665af1, for example with `git rev-parse HEAD`. My line numbers rest on the assumption that it was.
   - The `find` also turned up a GWT `LocalCache.java`. I took the main `guava/src` file without checking that the two differ.

3. **Next time:** Start with `grep` instead of `rg`. Verify the commit first. Read the eviction path beyond line 2666 if the question calls for it.

4. **Confidence:** Medium-high on the segment-count and per-segment-weight mechanics, because I read that code directly. Medium on the line numbers, since the pinned commit is unverified. My closing line "I did not run anything" was inaccurate. I ran grep and sed. I meant that I did not execute any Guava code.