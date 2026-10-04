**Repository:** `github.com/prometheus/client_golang`. I read the v1.20.0 tag (commit `73b811c54a628c7a7fe43005fb81351f64da9289`) of this repo directly. I did not separately confirm that Prometheus designates it as the "official" Go library.

**Default buckets:** If `Buckets` is nil or an empty slice, the histogram gets `DefBuckets`. These are `{.005, .01, .025, .05, .1, .25, .5, 1, 2.5, 5, 10}`, in seconds, and are meant for network-service response times (`prometheus/histogram.go:261-265`). The implicit `+Inf` bucket is added on top of these (`prometheus/histogram.go:390-391`).

**When the defaults are not applied:**
- **Explicit buckets:** You set a non-empty `Buckets` slice, and your own boundaries are used.
- **Native histogram enabled:** `NativeHistogramBucketFactor` is greater than 1 and `Buckets` is empty. The code is `if len(h.upperBounds) == 0 && opts.NativeHistogramBucketFactor <= 1 { h.upperBounds = DefBuckets }` (`prometheus/histogram.go:563-565`). In that case there are no classic buckets at all (`prometheus/histogram.go:392-396`).
- **Native plus classic buckets:** If you want both, you have to list the classic buckets explicitly in `Buckets` (`prometheus/histogram.go:394-396`).

**Uncertainty:** The tool output omitted lines 272-386 and 400-557, and I didn't read them. The answer rests on the doc comment and the constructor check shown above.