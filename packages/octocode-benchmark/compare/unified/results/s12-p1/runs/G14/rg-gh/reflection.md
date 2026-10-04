1. **Helped:** The `gh api -X GET "repos/prometheus/client_golang/contents/prometheus/histogram.go?ref=v1.20.0" -q .content | base64 -d | grep -n ...` call gave me pinned-tag source with line numbers in one step. The follow-up `sed -n '388,398p;555,570p'` showed the doc comment and the `NativeHistogramBucketFactor <= 1` condition, which answered the "when not applied" part.

2. **Did not help:**
   - My first call tried to write `h.go` and use `rg`. The write was blocked and `rg` wasn't installed, so the call was wasted. The task prompt had said local writes are blocked.
   - Every call printed `/dev/null: Operation not permitted`. My `2>/dev/null` attempt made the whole command print nothing, so I had to repeat it.
   - The `-q .sha` output in the first call was a blob SHA, not a commit SHA, and I never used it.

3. **Next time:** I'd pipe straight to `grep` or `sed` with no redirects, and use `grep` instead of `rg`. I'd also resolve the `v1.20.0` tag to a commit SHA with `gh api repos/.../git/ref/tags/v1.20.0`, so the evidence is pinned.

4. **Confidence:** High on the buckets and the native-histogram condition, since I read both in the source and the doc comment at the tag. Medium on "official" status, because I didn't check it, and the tag's commit SHA was not verified.