1. **Helped:** The second Bash call went straight to `guava/src/com/google/common/cache/` and ran one `rg` over `LocalCache.java` for `segmentShift|segmentMask|maxWeight|evictsBySize|maxSegmentWeight|concurrencyLevel`. That found the constructor logic and `CacheBuilder`'s default and setters in one pass. The following `sed -n` slices (`LocalCache.java:278-324`, `:1766-1769`, `:2655-2680`, and the `CacheBuilder.java` ranges) gave exact line numbers for the citations.

2. **Did not help:**
   - My first `find . -name LocalCache.java | head -1` returned the GWT super-source copy, which wasted a call.
   - Every Bash call printed `/dev/null: Operation not permitted`. This was noise, but `rg` and `sed` still worked.
   - `git log -1` failed for the same reason, so I could not confirm the checkout was at 4d41665af1. I said so in the answer.
   - I never opened the `Segment` constructor, so I did not verify how it sizes its table.

3. **Next time:** I would exclude `gwt` and `android` paths in the first search, or start from `guava/src`. I would also read the `Segment` constructor and use `git -C` or read `.git/HEAD` to check the commit.

4. **Confidence:** High on the mechanics, because I read the constructor, `segmentFor`, and `evictEntries` directly. Medium on the commit pin, since I never confirmed it.