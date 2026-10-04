The official Go instrumentation library is hosted at **github.com/prometheus/client_golang**.

**Default buckets in v1.20.0.** A histogram with no buckets configured gets `DefBuckets`: `.005, .01, .025, .05, .1, .25, .5, 1, 2.5, 5, 10`. The `+Inf` bucket is added implicitly. These are declared at `prometheus/histogram.go:265`. The comment at lines 261-264 says they are tailored to measure network service response time in seconds. The tag is v1.20.0, commit 73b811c.

**When the defaults are applied.** The `Buckets` doc comment (`histogram.go:391-392`) says a nil or empty `Buckets` is replaced by the defaults. The code at `histogram.go:563-565` is `if len(h.upperBounds) == 0 && opts.NativeHistogramBucketFactor <= 1 { h.upperBounds = DefBuckets }`.

**When they are not applied.**
- **Buckets are set explicitly.** A non-empty `Buckets` slice is used as given.
- **Native histogram is enabled.** If `NativeHistogramBucketFactor > 1` and `Buckets` is empty, the result is no regular buckets (`histogram.go:393-396`, `563`). To get both regular and native buckets, you have to list the regular ones explicitly.

I read only the parts of the file around these lines (the tool omitted lines 272-386, 400-557 and the rest of the file), so I haven't seen any other code that might change this.