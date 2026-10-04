**Repository:** `prometheus/client_golang` (https://github.com/prometheus/client_golang). The v1.20.0 tag resolves to `73b811c54a628c7a7fe43005fb81351f64da9289`.

**Default buckets:** A histogram with no buckets configured (`Buckets` nil or an empty slice) gets `DefBuckets`. At v1.20.0 that is `{.005, .01, .025, .05, .1, .25, .5, 1, 2.5, 5, 10}` (`prometheus/histogram.go:265`). The `+Inf` bucket is added implicitly. The doc comment says these are tuned for network-service response times in seconds (`prometheus/histogram.go:261-264`).

**When the defaults are not applied:** They are skipped when the histogram is configured as a native histogram, meaning `NativeHistogramBucketFactor > 1`. In that case the default is no classic buckets at all.
- The code is `if len(h.upperBounds) == 0 && opts.NativeHistogramBucketFactor <= 1 { h.upperBounds = DefBuckets }` (`prometheus/histogram.go:563-565`).
- The doc comment at `prometheus/histogram.go:391-396` says the same. To have both classic and native buckets, you must define the classic `Buckets` explicitly.
- If you set explicit non-empty `Buckets`, those are used instead of the defaults.

I read the source through the GitHub API at the v1.20.0 tag. I did not run any code.