Load when choosing replication, partitioning, consistency, distributed transactions or leader election, or integrity checks, deduplication and synchronization.

# Data systems: distributed data and integrity

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
| Consistent hashing | Partitioning with limited key movement | See [caching](data-systems-caching.md#eviction-and-admission) |
| Range partitioning | Ordered partition scans | Monotonic keys (timestamps) create a hot partition |
| Hash partitioning | Distribute keys evenly | Range scans touch every partition |
| Two-phase commit | Coordinate distributed transactions | Participants block if the coordinator fails after prepare |
| Saga pattern | Multi-service workflows as local transactions with compensating steps, coordinated by choreography or orchestration | No isolation: other readers see intermediate states |
| Outbox pattern | Persist outgoing events in the same transaction as the business data, instead of a dual write that loses events | The relay delivers at least once |
| Idempotency keys | A caller-supplied ID makes a retried request (payment, command) a no-op | Scope keys per client and operation; store the key with the first response and replay it for retries within the retry window |
| Leader election | Coordinate cluster ownership | Use leases with fencing tokens so a paused old leader cannot write |
| Anti-entropy repair | Synchronize divergent replicas | Usually Merkle-tree driven, runs in the background |

- **Delivery.** Guarantees and ordering scope are defined under [communication](concepts-communication.md#communication).
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
| Optimistic concurrency control | Detect conflicting updates | Version check at commit ([optimistic locking](concurrency-atomics.md#synchronization-and-atomic-operations)) |
| Version vectors | Track distributed version histories | One entry per replica, not per event |
| Consistent snapshots | Backup and recovery | Chandy–Lamport for distributed state; a backup counts only after a restore test |
