Load when choosing a lookup structure, index, sort, join or range-query structure, or when estimating complexity.

Read next: [sorting, aggregation and complexity](data-structures-sorting.md) for a sort, join, top-K or complexity class; [reverse lookup](data-structures-reverse-lookup.md) for a value-to-keys index and its [implementation](data-structures-reverse-lookup-code.md).

# Data structures and algorithms

Notation: n items, m key length, k results (or the K in top-K), α the inverse Ackermann function.

## Choose quickly

| Problem | Start with | Watch |
|---|---|---|
| Fast lookup | Hash map | Use hashing when ordering is unnecessary; use a keyed hash for untrusted keys |
| Reverse lookup | Bidirectional map | Store value → set of keys when values are not unique ([pattern](data-structures-reverse-lookup.md#reverse-lookup-pattern)) |
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
