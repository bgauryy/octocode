1. **Helped:** The `localGetFileContent` call on `LocalCache.java` lines 250-340 settled most of the answer. It showed the whole constructor, including the segment-count loop, `segmentShift`/`segmentMask`, and the `maxSegmentWeight` split. The first `localSearch` (regex on `segmentShift|segmentCount|concurrencyLevel`) located the constructor and the `CacheBuilder` javadoc quickly. Its `next` hint pointed at the right read.

2. **Did not help:**
   - The first search matched both `android/` and `guava/` copies, which doubled the output. It was also capped at 10 rows per file.
   - The third `localSearch` was a clumsy alternation. It returned a mangled `getMaximumWeight` snippet that I had to infer from, and it never showed `maximumSize()` fully. I never read the `Segment` class.
   - The code comment says "at least 10 entries" while the code uses `* 20L`. I reported the mismatch but did not check history for it.
   - I put a line number (`:1767`) on the `segmentFor` expression. The tool returned that snippet with the matched line at 1771 and the surrounding window starting at 1765. I should have verified that number.

3. **Next time:** I would scope the search to `guava/src` only, and use `matchString` reads on `maximumSize(` and `getMaximumWeight` for clean snippets. I would also read the `Segment` constructor to confirm how it enforces `maxSegmentWeight`.

4. **Confidence:** High on the segment-count and weight-split logic, since I read it directly. Medium on the exact line numbers for the `CacheBuilder` methods and `segmentFor`. Segment-level enforcement is unverified.