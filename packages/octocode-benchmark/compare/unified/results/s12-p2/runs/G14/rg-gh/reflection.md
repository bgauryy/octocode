1. **Helped:** The first `gh api -X GET repos/prometheus/client_golang/contents/prometheus/histogram.go?ref=v1.20.0` with the raw Accept header, piped through `grep -n`, found `DefBuckets` at line 265 and the `h.upperBounds = DefBuckets` line at 564 in one call. Reading the doc comment at 388-398 and the guard around 563 with `sed` in the second call confirmed the native-histogram exception.

2. **Did not help:**
- I downloaded the same file three times, twice in the first command and once in the second, instead of saving it once.
- Every call printed `/bin/bash: /dev/null: Operation not permitted`. It was harmless but noisy.
- I never resolved the v1.20.0 tag to a commit SHA, so the evidence is pinned only by tag name.
- Some of my line ranges were loose. The doc-comment range I cited, 393-398, missed the "If Buckets is left as nil" sentence, which starts around line 391.
- I did not check release notes or the PR history.

3. **Next time:** I would fetch the file once into a variable, or into a temp file if the sandbox allows it. I would also resolve the tag to a SHA with `gh api repos/.../git/ref/tags/v1.20.0` and run `grep -n` with context to get exact line ranges.

4. **Confidence:** High. The code and the doc comment agree, and both came from the v1.20.0 tag. The remaining risks are the loose line ranges and the unresolved SHA.