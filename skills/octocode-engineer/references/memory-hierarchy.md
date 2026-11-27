Load when a performance gap is not explained by complexity: cache levels, hardware latencies, locality and data layout.

# Memory: hierarchy and latency

## Memory hierarchy and latency

Rough orders of magnitude for planning; measure the target hardware before relying on them.

| Level or operation | Typical latency | Note |
|---|---|---|
| Register | Under 1 ns | Smallest, fastest |
| L1 cache hit | About 1 ns | Per core |
| L2 cache hit | About 3–5 ns | Larger, per core or cluster |
| L3 cache hit | About 10–40 ns | Often shared across cores |
| DRAM access | About 50–100 ns | Much larger; NUMA remote access costs more |
| NVMe SSD random read | About 10–100 µs | Persistent |
| Round trip in one datacenter | About 0.1–0.5 ms | Network |
| HDD seek | About 2–10 ms | Sequential reads are far cheaper than random |
| Round trip across a continent or ocean | About 50–150 ms | Bound by the speed of light in fibre |

Cache lines are 64 bytes on most x86-64 processors; Apple silicon reports 128 (`sysctl hw.cachelinesize`). In C++ use `std::hardware_destructive_interference_size` for padding rather than a literal.

| Optimization | Why it matters |
|---|---|
| Cache locality | Fewer expensive memory accesses |
| Spatial locality | Access nearby addresses |
| Temporal locality | Reuse recently accessed data |
| Cache-line alignment | Avoid unnecessary cache-line sharing |
| Data-oriented design | Lay data out for the common access pattern |
| AoS vs SoA | Choose object-oriented or field-oriented layout |
| Prefetching | Load data before it is needed |
| SIMD / vectorization | Process several elements per instruction |
| Zero-copy | Avoid unnecessary data transfers |
| NUMA locality | Minimize remote-node memory access |
| TLB optimization | Fewer address-translation misses |
| Branch prediction | Predictable branches avoid pipeline flushes |
| Batching | Amortize dispatch, syscall and synchronization costs |
| Memory alignment | Respect hardware and ABI alignment |
