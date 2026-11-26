Load when choosing or reviewing caches, storage engines, approximate counting, replication, partitioning, distributed transactions, integrity checks, data pipelines and governance, or data-management practice.

Read next: [caching](data-systems-caching.md) for eviction and read or write strategies; [probabilistic and streaming algorithms](data-systems-probabilistic.md) for approximate answers; [distributed data and integrity](data-systems-distributed.md) for replication, consistency, deduplication and synchronization; [data architecture and governance](data-systems-governance.md) for pipelines, schema evolution and retention.

# Data systems

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
