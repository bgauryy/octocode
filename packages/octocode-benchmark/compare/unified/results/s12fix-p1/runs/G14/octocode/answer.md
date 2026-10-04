**Repository:** `github.com/prometheus/client_golang`.

**Default buckets in v1.20.0:** a histogram with no buckets configured gets `DefBuckets`. That is `{.005, .01, .025, .05, .1, .25, .5, 1, 2.5, 5, 10}`, plus the implicit `+Inf` bucket. It is declared at `prometheus/histogram.go:265`. The doc comment at lines 261-264 says these are tailored to measure network-service response time in seconds. "None configured" means `Buckets` is nil or a zero-length slice (lines 391-392).

**When the defaults are not applied:**
- **Native histogram enabled.** The defaults are skipped when `NativeHistogramBucketFactor > 1`, which turns on native (sparse) buckets. The code is `if len(h.upperBounds) == 0 && opts.NativeHistogramBucketFactor <= 1 { h.upperBounds = DefBuckets }` at `histogram.go:563-565`. In that case the default is no classic buckets at all (lines 393-394). To get both classic and native buckets, you must set `Buckets` explicitly (lines 394-396).
- **Buckets set explicitly.** Any non-empty `Buckets` is used as given, so the defaults don't apply.

**Uncertainty:** I read lines 261-271, 387-397 and 558-570 at tag v1.20.0, which resolved to commit 73b811c. I didn't read the omitted middle sections.