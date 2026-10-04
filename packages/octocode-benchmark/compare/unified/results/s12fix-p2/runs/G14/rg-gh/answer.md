**Repository:** `prometheus/client_golang` (https://github.com/prometheus/client_golang). I read it through the GitHub API at tag `v1.20.0`.

**Default buckets:** if `Buckets` is nil or empty, the histogram gets `DefBuckets`. These are `{.005, .01, .025, .05, .1, .25, .5, 1, 2.5, 5, 10}` (`prometheus/histogram.go:265`). The `+Inf` bucket is added implicitly (doc comment on `HistogramOpts.Buckets`).

**When the defaults are not applied:** they are skipped when the histogram is configured as a native histogram, meaning `NativeHistogramBucketFactor > 1`.
- In that case the default is no classic buckets at all. The doc comment says "otherwise the default is no buckets" (`prometheus/histogram.go:393-397`).
- To get both classic and native buckets, you must set `Buckets` explicitly.
- The code check is `if len(h.upperBounds) == 0 && opts.NativeHistogramBucketFactor <= 1 { h.upperBounds = DefBuckets }` (`prometheus/histogram.go:563-565`).
- Explicitly supplied non-empty `Buckets` are also never replaced.

I did not check whether other code paths, such as vector constructors, change this behavior.