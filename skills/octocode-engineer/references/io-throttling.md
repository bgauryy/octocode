Load when limiting how fast or how wide work runs: rate limit, concurrency limit, backpressure, debounce or batch.

# I/O: throttling

## Throttling

| Mechanism | Applied by | Controls | Use |
|---|---|---|---|
| Rate limit | The provider or server | Requests per time window | Protect a resource ([algorithms](systems-reliability.md#rate-limiting-algorithms)) |
| Client-side throttle (pacing) | The caller | Its own send rate | Stay under a provider's quota; smooth bursts |
| Concurrency limit | Either side | Requests in flight | Protect real capacity; the admitted rate falls as latency rises (rule 1) |
| Backpressure | Consumer to producer | Buffer occupancy | Bounded queues, stream pause and resume, credit-based flow |
| Debounce | Event source | One run after a quiet period | Search boxes, resize, file-watcher saves |
| Throttle (event) | Event source | At most one run per interval | Scroll, progress updates |
| Request coalescing (single flight) | Cache or caller | Identical in-flight requests share one call | Cache-miss stampedes on hot keys |
| Batching | Caller | Several items per call | Amortize per-call cost; adds up to the batch window in latency |

Rules:

1. Prefer a concurrency limit for internal capacity: by Little's law, in-flight ≈ throughput × latency ([capacity](systems-capacity.md#capacity-and-estimation)). A rate limit ignores how long each request holds the resource.
2. Put the limit at the scarce resource and share it: process-local limiters multiply by the instance count. One limiter per provider credential across workers.
3. Pace from the provider's signals: `429` or `503` with `Retry-After`, rate-limit headers, secondary limits. A fixed `sleep` between calls is neither a limit nor adaptive.
4. Retries, retry budgets, fairness, load shedding and adaptive (AIMD) limits are in [reliability and overload](systems-reliability.md#reliability-and-overload).
5. Debounce with a trailing call so the last event is handled, and flush or cancel pending timers on dispose: a forgotten timer is a leak and can fire after shutdown.
6. Measure queue wait separately from service time: throttling shows as growing wait with flat service time.
