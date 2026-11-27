Load when handling overload, retries, timeouts, circuit breakers, idempotency or fencing, or choosing a rate-limiting algorithm.

# Systems: reliability, overload and rate limiting

## Reliability and overload

| Pattern | Purpose | Watch |
|---|---|---|
| Timeouts and deadlines | Bound every remote call | Propagate the caller's remaining deadline; an inner timeout must be shorter than the outer one |
| Retries with exponential backoff and jitter | Ride out transient failures | Full jitter: sleep a random time in [0, min(cap, base · 2^attempt)]; cap attempts |
| Retry budget | Keep retries from multiplying load | Retrying at every layer multiplies: three layers of three attempts is up to 27 calls |
| Circuit breaker | Stop calling a failing dependency | Closed → open on failures → half-open probes → closed |
| Bulkhead | Isolate failures | Separate pools or limits per dependency or tenant |
| Load shedding | Protect the system under overload | Drop low-priority work first; answer 429 or 503 with `Retry-After` |
| Backpressure | Slow producers to the consumer's pace | Mechanisms are under [throttling](io-throttling.md#throttling) |
| Graceful degradation | Serve a reduced result instead of failing | Stale cache, fewer features; tell the caller |
| Health checks | Route traffic only to working instances | Liveness (restart me) differs from readiness (send me traffic) |
| Idempotency | Make duplicates harmless | See [idempotency keys](data-systems-distributed.md#distributed-data-and-consistency) |
| Hedged requests | Cut tail latency by sending a duplicate after a delay | Hedge only idempotent reads, after about the p95, and cap the extra load |
| Adaptive concurrency limits | Adjust in-flight work from latency or error feedback | Needs a clean signal; tune slowly to avoid oscillation |
| Fencing tokens, leases | Time-bounded ownership that a stale holder cannot use | The resource must reject a token lower than the last one it saw ([leader election](data-systems-distributed.md#distributed-data-and-consistency)) |
| Failure domains, fault containment | Keep a fault inside one node, zone or cell | List what the domains still share; shared config or control plane is a common hole |
| Brownout | Switch off optional features under stress | Decide the feature order in advance |
| Poison message handling, quarantine | Move repeatedly failing messages or suspicious data aside | Cap redeliveries, then dead-letter with the error; alert on the queue |
| Failover and failback | Move service between redundant instances | Test both directions; split brain needs fencing |
| Disaster recovery | Restore after a major incident | Set RTO and RPO; only a restore drill proves a backup |

### Rate limiting algorithms

| Algorithm | Behaviour | Trade-off |
|---|---|---|
| Token bucket | Refill r tokens per second up to capacity b; each request spends one | Average rate r with bursts up to b |
| Leaky bucket (queue) | Drain at a constant rate | Smooth output; queued requests wait |
| Fixed window counter | Count per clock window | Simple; up to twice the limit across a window boundary |
| Sliding window log | Store each request time | Exact; memory per request |
| Sliding window counter | Weight the previous window's count by its overlap | Approximate, O(1) memory |
| Concurrency limit | Cap requests in flight | Tracks real capacity rather than a rate ([throttling](io-throttling.md#throttling)); adaptive variants adjust the cap from latency (AIMD) |
