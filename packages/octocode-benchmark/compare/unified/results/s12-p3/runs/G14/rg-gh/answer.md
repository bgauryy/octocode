**Repository:** `prometheus/client_golang` (https://github.com/prometheus/client_golang).

**Default buckets in v1.20.0:** If `Buckets` is nil or empty, the histogram gets `DefBuckets`. These are `{.005, .01, .025, .05, .1, .25, .5, 1, 2.5, 5, 10}` (`prometheus/histogram.go:265`). The `+Inf` bucket is added implicitly. The defaults are tuned for network-service response times in seconds (`prometheus/histogram.go:261-264`).

**When the defaults are not applied:** They are skipped when native (sparse) histogram buckets are in use, meaning `NativeHistogramBucketFactor > 1`. In that case the default is no classic buckets. To get both classic and native buckets, you must set `Buckets` explicitly. The `HistogramOpts.Buckets` doc comment says this (`prometheus/histogram.go:389-397`). The code is `if len(h.upperBounds) == 0 && opts.NativeHistogramBucketFactor <= 1 { h.upperBounds = DefBuckets }` (`prometheus/histogram.go:563-565`). Explicitly supplied non-empty buckets are also never replaced.

I read the file at the `v1.20.0` tag through the GitHub API. Line numbers come from that output, and I did not check them against a local checkout.