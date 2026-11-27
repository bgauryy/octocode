Load when an approximate answer is acceptable: membership, distinct counts, frequent items, quantiles, similarity or sampling over a stream.

# Data systems: probabilistic and streaming algorithms

## Probabilistic and streaming algorithms

Use these when an approximate answer cuts memory or processing by orders of magnitude, and define the acceptable error first ([Redis probabilistic types](https://redis.io/docs/latest/develop/data-types/probabilistic/)).

| Algorithm | What it solves | Guarantee or bound |
|---|---|---|
| Bloom filter | Membership test | No false negatives; about 9.6 bits per item for a 1% false-positive rate with k = (m/n) ln 2 hash functions; no deletion |
| Counting Bloom filter | Membership with deletion | Counters instead of bits (often 4 bits each); counter overflow |
| Cuckoo filter | Compact approximate membership with deletion | Delete only items that were inserted; inserts can fail when nearly full |
| HyperLogLog | Approximate distinct count | Standard error about 1.04/√m for m registers; Redis uses 12 KB for 0.81%; mergeable |
| Count-Min Sketch | Approximate item frequencies | Never underestimates (non-negative counts); error ≤ εN with probability 1 − δ for width ⌈e/ε⌉, depth ⌈ln(1/δ)⌉ |
| Space-Saving | Identify frequent items | k counters; count error at most N/k |
| Top-K | Track the most frequent elements | Redis Top-K uses HeavyKeeper: a count table plus a min-heap of K items |
| Reservoir sampling | Uniform sample of a stream of unknown length | O(k) memory for k samples |
| t-digest | Approximate percentiles | Most accurate at the tails; mergeable; no strict worst-case bound |
| KLL sketch | Mergeable approximate quantiles | Provable rank-error bound |
| MinHash | Estimate Jaccard set similarity | Error shrinks as 1/√(number of hashes); combine with LSH banding for candidate pairs |
| SimHash | Detect near-duplicate content | Compare fingerprints by Hamming distance |
| Sliding window | Rolling time-based aggregation | Exact windows store events or buckets; bucketed windows trade precision for memory |
| Exponential moving average | Smooth streaming metrics | s ← αx + (1 − α)s in O(1) memory; lags real changes |

Combine hosts by merging their sketches or histograms: the mean of per-host p99 values is not a p99 ([percentiles](systems-operations.md#observability-and-sre)).
