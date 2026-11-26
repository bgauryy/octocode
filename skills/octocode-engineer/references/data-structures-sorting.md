Load when choosing a sort, join, aggregation, top-K or range-sum structure, or when stating a complexity class.

# Data structures: sorting, aggregation and complexity

## Sorting, aggregation and data processing

| Algorithm | Best use | Cost | Note |
|---|---|---|---|
| Merge sort | Stable sorting, external sorting | O(n log n) in all cases | O(n) extra memory |
| Quick sort | General in-memory sorting | O(n log n) average, O(n²) worst | Not stable; introsort falls back to heap sort to cap the worst case |
| Heap sort | Bounded auxiliary memory | O(n log n) worst, O(1) extra | Not stable, poor locality |
| TimSort | Partially sorted data | O(n) best, O(n log n) worst | Stable; merges natural runs (Java object sorts; Python's list sort derives from it) |
| Radix sort | Fixed-width integer or string keys | O(d(n + b)) for d digits in base b | Not comparison-based; extra memory |
| External merge sort | Datasets larger than RAM | One pass to write sorted runs, then ⌈log_F(runs)⌉ merge passes at fan-in F | Bound by sequential I/O; tune run size and fan-in |
| Hash join | Equijoins | O(n + m) expected | Build side must fit memory or spill (grace hash join) |
| Sort-merge join | Pre-sorted inputs, range-friendly processing | Sorting O(n log n + m log m), merge O(n + m + output) | Duplicate keys on both sides can make the output n·m |
| Nested-loop join | Tiny inputs, or index lookups on the inner side | O(n·m) naive; O(n log m) with an inner B-tree | A planner picks it for a small outer side |
| Hash aggregation | `GROUP BY` | O(n) expected | Memory grows with the number of groups; spills to disk |
| Heap / priority queue | Top-K, scheduling | Push and pop O(log n), peek O(1), build O(n) | Top-K of n with a size-k min-heap: O(n log k) |
| Quickselect | k-th smallest element | O(n) average, O(n²) worst | Median of medians gives O(n) worst; C++ `std::nth_element` |
| Prefix sum | Static range sums | O(n) build, O(1) query | Any update forces a rebuild |
| Fenwick tree | Dynamic prefix and range sums | O(log n) update and query | Needs an invertible operation (sum, xor), not min/max over arbitrary ranges |
| Segment tree | Dynamic range queries and updates | O(log n); range updates with lazy propagation | About 2n–4n nodes |
| Union-find | Disjoint sets, connected components | O(α(n)) amortized with path compression and union by rank | No efficient split |

## Complexity reference

| Complexity | Typical example | Scaling |
|---|---|---|
| O(1) | Hash lookup | Constant on average |
| O(log n) | B-tree lookup | Excellent |
| O(√n) | Square-root decomposition | Moderate |
| O(n) | Full table scan | Linear |
| O(n log n) | Merge sort | The practical floor for comparison sorting |
| O(n²) | Nested comparisons | Expensive at scale |
| O(2ⁿ) | Exhaustive subset search | Impractical for large n |
| O(n!) | Brute-force permutations | Impractical beyond about a dozen items |

These are asymptotic models, not latency guarantees: skew and the hardware costs in [memory hierarchy and latency](memory-hierarchy.md#memory-hierarchy-and-latency) often dominate ([claim limits](analysis-proof.md#claim-limits)).
