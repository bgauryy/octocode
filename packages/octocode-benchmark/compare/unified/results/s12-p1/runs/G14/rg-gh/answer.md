**Repository:** `prometheus/client_golang` (https://github.com/prometheus/client_golang). I did not check the repo's "official" status separately. I'm going by it being the Prometheus org's Go client.

**Default buckets in v1.20.0:** a histogram with no buckets configured (`Buckets` nil or empty) gets `DefBuckets`. These are `{.005, .01, .025, .05, .1, .25, .5, 1, 2.5, 5, 10}`, plus the implicit `+Inf` bucket. The definition is at `prometheus/histogram.go:265`, and the fallback is applied at `prometheus/histogram.go:564`.

**When the defaults are not applied:** they are skipped when the histogram is configured as a native histogram, meaning `NativeHistogramBucketFactor > 1`. The code at lines 563–565 is `if len(h.upperBounds) == 0 && opts.NativeHistogramBucketFactor <= 1 { h.upperBounds = DefBuckets }`. In that case the default is no classic buckets at all. The `Buckets` field doc at lines 391–397 says the same thing. To get both classic and native buckets, you have to set `Buckets` explicitly.

**Uncertainty:** I read the file at tag `v1.20.0` through the GitHub API and did not check out the repo locally. I did not confirm the exact commit SHA of the tag.