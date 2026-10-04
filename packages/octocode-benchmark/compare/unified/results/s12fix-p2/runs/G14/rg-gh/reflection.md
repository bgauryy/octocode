**Helped:** The grep for `DefBuckets` on `histogram.go` at `?ref=v1.20.0` (the `gh api ... Accept: application/vnd.github.raw` call) found the definition at line 265, the doc comment at 393 and the assignment at 564 in one shot. The final `sed -n '555,572p'` call confirmed the exact condition (`NativeHistogramBucketFactor <= 1`). Fetching raw content at a pinned tag meant no local checkout was needed.

**Did not help:**
- My first call also grepped `metric.go`, which returned nothing and was a wasted guess.
- I downloaded `histogram.go` three times. One fetch saved to a temp file would have covered all the greps and the sed.
- Every call printed `/bin/bash: /dev/null: Operation not permitted`. It was harmless but noisy, and it made the first call exit with code 1 even though it produced output.
- The `cd /tmp` was pointless.
- I could not check call sites outside `histogram.go`, such as vector constructors.

**Next time:** Fetch once to a file, then use `rg -n` with context. Use `gh api` search or the tree to locate the file instead of guessing `metric.go`.

**Confidence:** High on the repository, the default values and the native-histogram exception, since I read the code and doc comment at the v1.20.0 tag. The line range 393-397 for the doc comment is slightly inferred from the sed output. I did not look at the vector constructors.