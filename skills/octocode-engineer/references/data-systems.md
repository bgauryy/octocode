# Data systems

Load when choosing or reviewing caches, storage engines, approximate counting, replication, partitioning, distributed transactions, integrity checks, data pipelines and governance, or data-management practice.

## Choose quickly

| Problem | Start with | Watch |
|---|---|---|
| Membership precheck | Bloom filter | False positives are possible, false negatives are not |
| Distinct counts at scale | HyperLogLog | Use an exact set when precision is mandatory |
| Frequent-item tracking | Count-Min Sketch for frequencies; Space-Saving or HeavyKeeper for the top items | A sketch alone estimates counts but does not list heavy hitters: pair it with a heap |
| Write-heavy storage | LSM tree | Account for compaction and read amplification |
| Read-heavy cache | LRU / W-TinyLFU | Benchmark against the real access trace |
| Streaming percentiles | t-digest / KLL | Approximation trades accuracy for memory; merge sketches, never average percentiles |
| Distributed key placement | Rendezvous or consistent hashing | Minimize key movement when nodes change |
| Distributed consensus | Raft | Consensus does not replace application consistency design |
| Concurrent offline updates | CRDT | Merge semantics must match the data model |
| Deduplicate files | Content hashing | Verify collisions as the integrity goal requires |
| Publish an event with a database change | Outbox pattern | Consumers must be idempotent: delivery is at least once |
| Retried mutations | Idempotency keys | Store key and result for the whole retry window |

## Caching

### Eviction and admission

| Policy | Purpose | Trade-off |
|---|---|---|
| LRU | Evict least recently used | O(1) with a hash map and a doubly linked list; one large scan flushes it |
| LFU | Evict least frequently used | Frequency bookkeeping (O(1) with frequency buckets); stale popular items linger without aging |
| FIFO | Evict oldest inserted | Simple, ignores popularity |
| TTL eviction | Expire stale records | Needs a sweep or lazy expiry check; synchronized expiry causes stampedes, so add jitter |
| ARC | Adapt between recency and frequency | More complex, keeps ghost lists |
| 2Q | Protect the cache from one-time scans | Multiple queues to size |
| TinyLFU / W-TinyLFU | Frequency-based admission (Caffeine) | Approximate counts in a sketch with periodic aging |
| CLOCK | Low-overhead replacement with a reference bit | Approximates recency; variants run OS page caches and PostgreSQL's buffer clock sweep |
| Consistent hashing | Distribute keys across nodes | A node change moves about 1/N of the keys; load is uneven without virtual nodes |
| Rendezvous hashing | Stable node assignment by highest hash(key, node) | O(N) scoring per lookup; minimal movement without virtual nodes |

### Read and write strategies

| Strategy | How it works | Trade-off |
|---|---|---|
| Cache-aside | The application loads on a miss and invalidates on write | Stale reads and invalidation races; misses stampede the store |
| Read-through | The cache loads from the store itself | Couples the cache to the store's API |
| Write-through | Write cache and store together | Higher write latency |
| Write-back (write-behind) | Buffer writes in the cache, flush later | Data loss on crash without a durable buffer |
| Write-around | Write only the store | The next read misses |
| Stampede protection | Request coalescing (single flight), early refresh, jittered TTLs | More moving parts; essential for hot keys |

Design invalidation explicitly: state how stale a read may be, who invalidates, and what happens when invalidation is lost.

## Storage engines and database internals

| Technique | Application | Note |
|---|---|---|
| LSM tree | Write-heavy databases | Writes go to the WAL and memtable, flush to SSTables, merge by compaction |
| SSTable | Immutable sorted disk files | Usually carries an index block and a Bloom filter |
| MemTable | In-memory write buffering | Often a skip list |
| WAL (write-ahead log) | Crash recovery | Log before applying; the fsync policy sets the durability-latency trade-off |
| Leveled compaction | Control read and space amplification | Higher write amplification |
| Size-tiered compaction | Favor write throughput | Higher space and read amplification |
| MVCC | Concurrent transactions and snapshots | Readers do not block writers; old versions need vacuum or garbage collection |
| Copy-on-write | Snapshots, immutable updates | Copy-on-write B-trees and file systems; write amplification on small updates |
| Append-only log | Event storage, audit trails | Needs retention or compaction |
| Extendible hashing | Dynamic disk-based hash indexes | The directory doubles as buckets split |
| Linear hashing | Incremental hash-index growth | Splits one bucket at a time |
| Columnar storage | Analytics and compression | Point updates and whole-row reads cost more |
| Dictionary encoding | Compress repetitive values | Best at low cardinality |
| Run-length encoding (RLE) | Compress repeated sequences | Best on sorted columns |
| Delta encoding | Store differences between values | Timestamps, sorted IDs |
| Bit packing | Compact integer storage | Pairs with delta encoding |
| Zone maps | Skip irrelevant blocks | Min/max per block; effective only when data is clustered |

**Amplification.** Read amplification is I/O per logical read, write amplification is bytes written to storage per byte written by the user, space amplification is stored bytes per logical byte. An engine cannot minimize all three at once (the RUM conjecture), so measure all three under the real workload.

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

Percentiles do not average: p99 across hosts comes from merged histograms or sketches, never from the mean of each host's p99.

## Distributed data and consistency

| Algorithm or pattern | Purpose | Watch |
|---|---|---|
| Raft | Consensus through an elected leader and a replicated log | 2f + 1 nodes tolerate f failures; every commit needs a majority |
| Paxos / Multi-Paxos | Consensus and replicated state | Multi-Paxos keeps a stable leader to amortize rounds |
| Gossip protocol | Disseminate cluster state | Eventual; spreads in about O(log N) rounds |
| CRDT | Merge concurrent updates under defined rules | Merges must be commutative, associative and idempotent; metadata and tombstones grow |
| Vector clocks | Track causality and detect concurrent updates | Size grows with participants |
| Lamport clocks | Logical event ordering | a → b implies L(a) < L(b), not the converse: cannot detect concurrency |
| Hybrid logical clocks | Logical ordering with a physical-time component | Stays close to wall clock; still depends on bounded skew |
| Quorum reads and writes | Configurable replication consistency | R + W > N makes read and write sets overlap; overlap alone is not linearizability (sloppy quorums, concurrent writes) |
| Merkle trees | Compare and reconcile replicated datasets | Finds differing ranges with O(log n) hash comparisons each |
| Consistent hashing | Partitioning with limited key movement | See [caching](#eviction-and-admission) |
| Range partitioning | Ordered partition scans | Monotonic keys (timestamps) create a hot partition |
| Hash partitioning | Distribute keys evenly | Range scans touch every partition |
| Two-phase commit | Coordinate distributed transactions | Participants block if the coordinator fails after prepare |
| Saga pattern | Multi-service workflows as local transactions with compensating steps, coordinated by choreography or orchestration | No isolation: other readers see intermediate states |
| Outbox pattern | Persist outgoing events in the same transaction as the business data, instead of a dual write that loses events | The relay delivers at least once |
| Idempotency keys | A caller-supplied ID makes a retried request (payment, command) a no-op | Scope keys per client and operation; keep results for the retry window |
| Leader election | Coordinate cluster ownership | Use leases with fencing tokens so a paused old leader cannot write |
| Anti-entropy repair | Synchronize divergent replicas | Usually Merkle-tree driven, runs in the background |

- **Delivery.** At-most-once loses messages, at-least-once duplicates them, and "exactly once" is at-least-once plus idempotent or transactional processing; guarantees and ordering scope are defined under [communication](concepts.md#communication).
- **Partitions.** During a network partition a replicated system gives up either consistency or availability (CAP); without a partition it still trades latency for consistency (PACELC).
- **Time.** Wall clocks jump and skew: order events with logical clocks or a consensus log, not timestamps alone.

## Integrity, deduplication and synchronization

| Technique | Purpose | Note |
|---|---|---|
| SHA-256 / BLAKE3 | Content hashing, integrity | Detects tampering only when the expected hash comes from a trusted source; authenticity needs an HMAC or signature |
| Content-addressable storage | Identify data by its hash | Deduplicates by construction |
| Merkle DAG | Track shared immutable objects | Git and IPFS |
| Rolling hash | Efficient substring and chunk matching | Rabin–Karp |
| Rabin fingerprinting | Content-defined chunking | Chunk boundaries survive insertions |
| rsync algorithm | Efficient delta synchronization | Weak rolling checksum, then a strong hash to confirm |
| Checksums / CRC | Detect accidental corruption | Not protection against a deliberate change |
| Reed–Solomon coding | Erasure-coded fault tolerance | k data plus m parity shards survive the loss of any m |
| WAL + checkpointing | Recovery after crashes | Checkpoints bound replay time |
| Optimistic concurrency control | Detect conflicting updates | Version check at commit; retries under contention |
| Version vectors | Track distributed version histories | One entry per replica, not per event |
| Consistent snapshots | Backup and recovery | Chandy–Lamport for distributed state; a backup counts only after a restore test |

## Data architecture and governance

| Concept | Role | Watch |
|---|---|---|
| OLTP / OLAP separation | Keep transactional and analytical workloads apart | Analytical scans on the primary steal its I/O and locks |
| Change data capture (CDC) | Stream committed changes from the database log | Per-key ordering, the snapshot-to-log handoff, schema changes in the stream |
| Data contracts | Producer-consumer expectations: schema, semantics, freshness, owner | Enforce in the producer's build, not by consumer complaints |
| Data catalog, lineage, provenance | Find data assets; trace origins and transformations; prove where data came from | Lineage drives impact analysis and deletion propagation |
| Data quality rules | Validate completeness, consistency and accuracy | Run at ingestion and quarantine failures instead of dropping them silently |
| Schema evolution | Change schemas safely | Backward- or forward-compatible by rule; see [API contracts](systems-and-delivery.md#api-and-interface-contracts) |
| Retention policies | Govern storage and deletion | Deletion must reach replicas, caches, backups and derived data; legal holds override it |
| Bitemporal data | Track valid time (when true in the world) and transaction time (when recorded) | Corrections without losing what was known when |
| Slowly changing dimensions | Preserve analytical history (type 1 overwrites, type 2 adds a row per version) | Type 2 needs effective dates and a current flag |
| Event-time processing, watermarks | Process by when events happened; a watermark asserts that earlier event times are complete | Too early drops late events, too late adds latency |
| Late event handling | Accept, update or side-output delayed input | Set allowed lateness explicitly |
| Reprocessing and replay | Rebuild derived state from the log | Needs retention, deterministic transforms and idempotent sinks |
| Reconciliation | Compare data between systems | Schedule it; it catches what every other check missed |

## Data management practices

| Area | Practice |
|---|---|
| Schema design | Normalization, deliberate denormalization, constraints |
| Index design | Composite, covering, partial and expression indexes |
| Data lifecycle | Retention, TTL, archival, deletion |
| Reliability | Backups, restore tests, replication |
| Consistency | Transactions, MVCC, conflict resolution |
| Scalability | Partitioning, sharding, batching |
| Observability | Query plans, latency, cache hit rate |
| Security | Access controls, encryption, audit logs |
| Data quality | Validation, uniqueness, reconciliation |
| Evolution | Online schema migrations, versioning |

## Rules worth remembering

1. Measure before optimizing: query plans, p95/p99 latency, CPU, memory and I/O.
2. Index access patterns, not every column: indexes cost storage and write overhead.
3. Know the workload: read-heavy, write-heavy and analytical systems need different designs.
4. Use approximate algorithms deliberately, with an accepted error rate.
5. Design cache invalidation explicitly, with freshness and consistency requirements.
6. Assume retries and duplicates: make distributed mutations idempotent.
7. Measure read, write and space amplification in storage engines.
8. Plan for growth: skew, hot keys, cardinality and partition rebalancing.
9. Protect correctness first: constraints, transactions, checksums and recovery procedures.
10. Benchmark realistic data: production-like distributions and concurrent access.

Foundations worth mastering first: hash maps, B+ trees, inverted indexes, LSM trees, Bloom filters, LRU/LFU, consistent hashing, MVCC, WAL and Raft. Together they explain much of how databases and backend infrastructure work.
