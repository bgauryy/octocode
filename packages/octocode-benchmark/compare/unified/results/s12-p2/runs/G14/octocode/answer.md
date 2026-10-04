**Repository:** `github.com/prometheus/client_golang`.

**Default buckets (v1.20.0):** If you configure none, a histogram gets `DefBuckets`: `{.005, .01, .025, .05, .1, .25, .5, 1, 2.5, 5, 10}`. That is `prometheus/histogram.go:265`, and the implicit `+Inf` bucket is added on top. The doc comment at lines 261-264 says these are tailored to response times in seconds.

**When the defaults are not applied:**
- You set `Buckets` explicitly to a non-empty slice. The doc at lines 391-392 says only a nil or zero-length `Buckets` is replaced by defaults.
- You enable native (sparse) histogram buckets. The doc at lines 393-396 says the default is then no classic buckets. If you want both kinds, you must define the regular buckets explicitly.
- The code at line 563 confirms this. `h.upperBounds = DefBuckets` is applied only if `len(h.upperBounds) == 0 && opts.NativeHistogramBucketFactor <= 1`. So a `NativeHistogramBucketFactor` greater than 1 means the defaults are skipped.

All line numbers are from tag v1.20.0 (commit 73b811c5). I did not read the omitted code between lines 272-386 and 400-557.