# Data structures and algorithms

Load when choosing a lookup structure, index, sort, join or range-query structure, or when estimating complexity. Notation: n items, m key length, k results (or the K in top-K), α the inverse Ackermann function.

## Choose quickly

| Problem | Start with | Watch |
|---|---|---|
| Fast lookup | Hash map | Use hashing when ordering is unnecessary; use a keyed hash for untrusted keys |
| Reverse lookup | Bidirectional map | Store value → set of keys when values are not unique ([pattern](#reverse-lookup-pattern)) |
| SQL equality and range queries | B+ tree / B-tree | Index the actual query patterns, not every column |
| JSON or array search | GIN | Supported operators depend on the database and operator class |
| Prefix search | Trie / ART | Autocomplete, routing tables |
| Approximate vector search | HNSW | Tune recall, memory and latency together |
| Large-dataset sorting | External merge sort | Optimize sequential disk and network I/O |
| Top-K of a collection | Size-k heap | O(n log k); quickselect when order inside the K does not matter |
| Dynamic range sums | Fenwick tree | Segment tree for min/max or range updates |
| Connectivity, grouping | Union-find | No efficient split or delete |

## Lookup, search and indexing

| Structure | Best use | Typical cost | Caveat |
|---|---|---|---|
| Hash map | Key → value lookup | O(1) average | O(n) worst under collisions; no ordering |
| Bidirectional map | Key ↔ value lookup | O(1) average each way | Both maps must change in one update path |
| Reverse / secondary index | Find records by non-primary attributes | The underlying index's cost | Every index adds write cost and storage |
| Inverted index | Full-text search, tags | Term lookup O(1) average with a hashed dictionary, O(m) or O(log n) with a trie, FST or sorted dictionary; plus posting-list length | Posting-list intersection dominates multi-term queries |
| B-tree / B+ tree | Database indexes, equality and ranges, ordered reads | O(log n) with high fan-out, so few pages per lookup | PostgreSQL's default index type |
| Red-black tree | Ordered maps and sets in memory | O(log n) | Pointer-heavy, weaker cache locality than a B-tree |
| AVL tree | Read-heavy ordered data | O(log n); stricter balance gives shorter paths | More rotations on writes |
| Skip list | Concurrent ordered indexes, LSM memtables | O(log n) expected | Probabilistic balance, extra pointers |
| Trie / radix tree | Prefix matching, IP longest-prefix routing | O(m) | Memory per node unless paths are compressed (radix) |
| ART (adaptive radix tree) | Memory-efficient in-memory indexing | O(m) | Node sizes adapt to child count (4, 16, 48, 256) |
| Binary search | Sorted random-access arrays | O(log n) | Compute the midpoint as `lo + (hi - lo) / 2` to avoid overflow |
| Interpolation search | Near-uniform sorted numeric keys | O(log log n) average on uniform data; O(n) worst | Skewed data destroys the average |
| Hash index | Exact-match database queries | O(1) expected | Equality only: no ranges or ordering |
| GIN | JSON, arrays, full-text (multi-valued columns) | Query-dependent; fast lookups | Updates cost more than B-tree updates |
| GiST / SP-GiST / R-tree | Geospatial, multidimensional, nearest-neighbour search | Data-dependent; no worst-case bound for R-trees | GiST and SP-GiST are frameworks: the operator class decides what is supported |
| BRIN | Very large tables whose physical order tracks the column (time series) | Scans only matching block ranges | Tiny, but useless when physical order does not correlate with the column |
| Bitmap index | Low-cardinality analytical columns | Fast AND/OR/NOT over bitmaps | Expensive under concurrent updates; PostgreSQL has no stored bitmap index, only bitmap scans built from other indexes |
| LSH | Approximate similarity search | Sub-linear, data-dependent | Recall depends on the hash family and table count |
| HNSW | Approximate vector nearest-neighbour search | Tunable (`M`, `ef`); roughly logarithmic in practice, no fixed guarantee | Memory-heavy; deletes and recall need tuning |

For database index types, B-tree is the default and fits most equality and range comparisons; GIN, GiST, SP-GiST and BRIN serve other query patterns ([PostgreSQL index types](https://www.postgresql.org/docs/current/indexes-types.html)).

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

These are asymptotic models, not latency guarantees: skew and the hardware costs in [memory and performance](memory-and-performance.md#memory-hierarchy-and-latency) often dominate ([claim limits](analysis.md#claim-limits)).

## Reverse lookup pattern

A TypeScript bidirectional index that allows several keys per value and keeps both directions in step.

```ts
class BiIndex<K, V> {
  private forward = new Map<K, V>();
  private reverse = new Map<V, Set<K>>();

  set(key: K, value: V): void {
    this.delete(key);

    this.forward.set(key, value);

    if (!this.reverse.has(value)) {
      this.reverse.set(value, new Set());
    }
    this.reverse.get(value)!.add(key);
  }

  get(key: K): V | undefined {
    return this.forward.get(key);
  }

  getKeys(value: V): ReadonlySet<K> {
    return this.reverse.get(value) ?? new Set<K>();
  }

  delete(key: K): boolean {
    if (!this.forward.has(key)) return false;

    const value = this.forward.get(key)!;
    this.forward.delete(key);

    const keys = this.reverse.get(value)!;
    keys.delete(key);

    if (keys.size === 0) {
      this.reverse.delete(value);
    }
    return true;
  }
}

// Usage
const users = new BiIndex<number, string>();
users.set(1, "admin");
users.set(2, "admin");
users.set(3, "editor");

console.log([...users.getKeys("admin")]); // [ 1, 2 ]
```

- **Cost.** `set`, `get` and `delete` are O(1) average; getting the reverse set is O(1) average plus O(r) to enumerate r keys. Space is O(n) plus the second map.
- **Reassignment.** `set` deletes the old mapping first, so moving key 1 from `"admin"` to `"editor"` removes it from the `"admin"` set, and an emptied set is dropped.
- **Live view.** `getKeys` returns the internal set. `ReadonlySet` blocks mutation only at compile time, and later writes show through. Return `new Set(keys)` when callers need a snapshot.
- **Equality.** `Map` compares keys with SameValueZero: objects match by identity, not by content.
- **Concurrency.** One event loop makes each method atomic in JavaScript; in a threaded language guard both maps with one lock.
