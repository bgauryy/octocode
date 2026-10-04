**Repository:** `prometheus/client_golang` (https://github.com/prometheus/client_golang).

**Default buckets in v1.20.0:** When `Buckets` is nil or empty, the histogram gets `DefBuckets`. These are `{.005, .01, .025, .05, .1, .25, .5, 1, 2.5, 5, 10}`, defined at `prometheus/histogram.go:265`. The upper bound is in seconds, and the implicit `+Inf` bucket is added on top (`histogram.go:390`). The doc comment says they are tailored to broadly measure the response time of a network service.

**When the defaults are not applied:**
- **Buckets are set explicitly.** If `Buckets` is non-empty, it is used as given.
- **Native histogram is enabled.** If `NativeHistogramBucketFactor > 1` and `Buckets` is empty, the default is no classic buckets. The code at `histogram.go:564` is guarded by `len(h.upperBounds) == 0 && opts.NativeHistogramBucketFactor <= 1`. The doc comment at `histogram.go:393-398` says the same. To get both classic and native buckets, you have to list the classic buckets explicitly.

I read all of this from the v1.20.0 tag. I did not check other versions.