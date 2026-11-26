Load when choosing a cache eviction or admission policy, or a read and write strategy (cache-aside, read-through, write-through, write-behind).

# Data systems: caching

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
