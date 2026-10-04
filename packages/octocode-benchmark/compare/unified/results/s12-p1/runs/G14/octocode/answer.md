The official Go instrumentation library is in the **`prometheus/client_golang`** repository (https://github.com/prometheus/client_golang). I read the source at tag `v1.20.0`, commit 73b811c.

**Default buckets.** A histogram with no buckets configured gets `DefBuckets`, defined at `prometheus/histogram.go:265`:

`{.005, .01, .025, .05, .1, .25, .5, 1, 2.5, 5, 10}`

The doc comment at lines 261-264 says these are tailored to broadly measure the response time, in seconds, of a network service. The implicit `+Inf` bucket is added as well (`histogram.go:390`).

**When the defaults apply.** They apply when `Buckets` is nil or an empty slice (`histogram.go:391-392`). The code at `histogram.go:563-565` is `if len(h.upperBounds) == 0 && opts.NativeHistogramBucketFactor <= 1 { h.upperBounds = DefBuckets }`.

**When they are not applied.**
- **Native histogram enabled.** If `NativeHistogramBucketFactor > 1`, the default is no regular buckets at all (`histogram.go:393-396`, `563`). To get both regular and native buckets, you have to set `Buckets` explicitly.
- **Buckets set explicitly.** Any non-empty `Buckets` slice is used as given.

I did not read lines 272-386 or 400-557 of the file.