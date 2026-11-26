Load when estimating capacity, availability, queueing or load: formulas and back-of-envelope numbers.

# Systems: capacity and estimation

## Capacity and estimation

| Rule | Formula | Worked number |
|---|---|---|
| Little's law | L = λW (in flight = arrival rate × time in system) | 200 req/s at 50 ms keeps 10 requests in flight |
| Queueing delay | M/M/1: W = S / (1 − ρ) for service time S and utilization ρ | ρ = 0.5 doubles latency, ρ = 0.9 makes it 10×, ρ = 0.99 makes it 100× |
| Amdahl's law | Speedup = 1 / ((1 − p) + p/n) | p = 0.95 caps speedup at 20× however many cores |
| Tail at scale | P(one request slow) = 1 − (1 − p)^N for fan-out N | p = 1%, N = 100 gives 63% of requests slow |
| Serial availability | Multiply the availabilities of required dependencies | Three at 99.9% give about 99.7% |
| Request rate | Requests per day / 86,400 | 1 million per day ≈ 11.6 per second average; plan for the peak, not the mean |
| Gustafson's law | Scaled speedup = (1 − p) + p·n, when the problem grows with n | p = 0.95, n = 64 gives about 60.9×: bigger problems, not faster small ones |
| Universal scalability law | C(n) = n / (1 + α(n − 1) + βn(n − 1)), α contention, β coherency | Any β > 0 makes throughput peak and then fall as nodes are added; fit α, β and the peak with `capacity.mjs` (below) |
| Availability | A = uptime / (uptime + downtime) = MTBF / (MTBF + MTTR) | MTBF 1,000 h with MTTR 1 h gives 99.9%; halving MTTR buys as much as doubling MTBF |

To fit the universal scalability law, save one `concurrency,throughput` row per load-test run (at least three concurrency levels, one throughput unit), then run `node <skill-folder>/scripts/capacity.mjs usl runs.csv` ([capacity.mjs](../scripts/capacity.mjs)). Read `alpha` (contention), `beta` (coherency) and `peak`: `peak.bestInteger` is the concurrency with the most throughput, and `peak.extrapolated: true` puts it beyond the largest measured level, so measure near it before sizing on it. `peak: null` means β = 0, with throughput approaching `ceiling` (λ/α). Rely on the fit only when `fit.dof` is at least 1 and `fit.maxRelativeError` is within the run-to-run noise. `constrained` names a term held at 0, and exit 2 names a bad row.

Scale horizontally (more instances) for stateless work and vertically (bigger nodes) until a node's limit; autoscale on a leading signal such as queue depth or concurrency, not CPU alone. Bound accepted work with admission control, resource quotas and capacity reservations; isolate workloads (pools, cells, per-tenant limits) so a noisy neighbour cannot take shared capacity; spread hot partitions by salting or splitting keys.

| Availability | Downtime per year | Downtime per month (730 h) |
|---|---|---|
| 99% | 3.65 days | 7.3 hours |
| 99.9% | 8.76 hours | 43.8 minutes |
| 99.99% | 52.6 minutes | 4.4 minutes |
| 99.999% | 5.26 minutes | 26 seconds |

Powers of two: 2¹⁰ ≈ 10³, 2²⁰ ≈ 10⁶, 2³⁰ ≈ 10⁹, 2³² ≈ 4.3 × 10⁹. Hardware latencies are in [memory hierarchy and latency](memory-hierarchy.md#memory-hierarchy-and-latency).
