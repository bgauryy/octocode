# Systems and delivery

Load when the question is about overload, retries, rate limits, capacity, API contracts, security and identity, testing, observability and SRE, deployment, code design or debugging method.

## Choose quickly

| Problem | Start with | Watch |
|---|---|---|
| A dependency is slow or down | Timeout + bounded retries with jittered backoff + circuit breaker | Retry only idempotent operations or ones carrying an idempotency key |
| Traffic exceeds capacity | Admission control: rate limit, concurrency limit, load shedding | Reject early and cheaply; queues only hide overload |
| Bursty clients | Token bucket | The bucket size is the allowed burst |
| Schema or API change in production | Expand–contract migration | Old and new versions must run side by side |
| Risky deploy | Feature flag or canary with automatic rollback | Define the rollback signal before the rollout |
| Unknown production behaviour | RED metrics per service, USE metrics per resource, traces across calls | Alert on symptoms users feel, not on every cause |
| A bug you cannot explain | Reproduce, minimize, bisect | Change one variable per experiment |

## Reliability and overload

| Pattern | Purpose | Watch |
|---|---|---|
| Timeouts and deadlines | Bound every remote call | Propagate the caller's remaining deadline; an inner timeout must be shorter than the outer one |
| Retries with exponential backoff and jitter | Ride out transient failures | Full jitter: sleep a random time in [0, min(cap, base · 2^attempt)]; cap attempts |
| Retry budget | Keep retries from multiplying load | Retrying at every layer multiplies: three layers of three attempts is up to 27 calls |
| Circuit breaker | Stop calling a failing dependency | Closed → open on failures → half-open probes → closed |
| Bulkhead | Isolate failures | Separate pools or limits per dependency or tenant |
| Load shedding | Protect the system under overload | Drop low-priority work first; answer 429 or 503 with `Retry-After` |
| Backpressure | Slow producers to the consumer's pace | Bounded queues, credit-based flow control |
| Graceful degradation | Serve a reduced result instead of failing | Stale cache, fewer features; tell the caller |
| Health checks | Route traffic only to working instances | Liveness (restart me) differs from readiness (send me traffic) |
| Idempotency | Make duplicates harmless | See [idempotency keys](data-systems.md#distributed-data-and-consistency) |
| Hedged requests | Cut tail latency by sending a duplicate after a delay | Hedge only idempotent reads, after about the p95, and cap the extra load |
| Adaptive concurrency limits | Adjust in-flight work from latency or error feedback | Needs a clean signal; tune slowly to avoid oscillation |
| Fencing tokens, leases | Time-bounded ownership that a stale holder cannot use | The resource must reject a token lower than the last one it saw ([leader election](data-systems.md#distributed-data-and-consistency)) |
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
| Concurrency limit | Cap requests in flight | Tracks real capacity; adaptive variants adjust the cap from latency (AIMD) |

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
| Universal scalability law | C(n) = n / (1 + α(n − 1) + βn(n − 1)), α contention, β coherency | Any β > 0 makes throughput peak and then fall as nodes are added; fit α and β from load tests |
| Availability | A = uptime / (uptime + downtime) = MTBF / (MTBF + MTTR) | MTBF 1,000 h with MTTR 1 h gives 99.9%; halving MTTR buys as much as doubling MTBF |

Scale horizontally (more instances) for stateless work and vertically (bigger nodes) until a node's limit; autoscale on a leading signal such as queue depth or concurrency, not CPU alone. Bound accepted work with admission control, resource quotas and capacity reservations; isolate workloads (pools, cells, per-tenant limits) so a noisy neighbour cannot take shared capacity; spread hot partitions by salting or splitting keys.

| Availability | Downtime per year | Downtime per month (730 h) |
|---|---|---|
| 99% | 3.65 days | 7.3 hours |
| 99.9% | 8.76 hours | 43.8 minutes |
| 99.99% | 52.6 minutes | 4.4 minutes |
| 99.999% | 5.26 minutes | 26 seconds |

Powers of two: 2¹⁰ ≈ 10³, 2²⁰ ≈ 10⁶, 2³⁰ ≈ 10⁹, 2³² ≈ 4.3 × 10⁹. Hardware latencies are in [memory and performance](memory-and-performance.md#memory-hierarchy-and-latency).

## API and interface contracts

- **Specify the contract.** Units, ranges, nullability, ordering, limits, error codes, idempotency, consistency (read-your-writes or eventual) and thread safety.
- **Evolve additively.** Add optional fields and new endpoints; never change a field's meaning or type in place. Version when a breaking change is unavoidable and run both versions until callers move.
- **Paginate with cursors.** Offset pagination scans the skipped rows and shifts under inserts; an opaque cursor over a stable sort key does neither.
- **Respect HTTP method semantics.** GET, HEAD, OPTIONS and TRACE are safe; those plus PUT and DELETE are idempotent (RFC 9110); POST and PATCH are neither, so they need idempotency keys to be retried safely.
- **Return actionable errors.** A stable machine-readable code, a human message, the failing field, and whether a retry can succeed.
- **Bound inputs and outputs.** Maximum sizes, batch limits and timeouts are part of the contract, not implementation details.
- **Define the contract first.** API-first or schema-first: OpenAPI for HTTP, AsyncAPI for events, Protocol Buffers, Avro or JSON Schema for payloads, a schema registry that rejects incompatible event schemas.
- **Test compatibility.** Consumer-driven contract tests in the provider's build; publish a deprecation policy with dates and usage telemetry before removing anything.
- **Upcast old data.** Convert stored events or documents of an older shape on read, so readers handle one shape.
- **Negotiate capabilities.** Let client and server agree on protocol version and features instead of assuming them.
- **Use conditional requests.** An ETag with `If-None-Match` saves a transfer (304); with `If-Match` it rejects a lost update (412).
- **Standardize errors.** Problem Details (RFC 9457, `application/problem+json`) gives type, title, status, detail and instance.
- **Deduplicate requests.** Store the idempotency key with the first response and replay it for retries; keep pagination stable under concurrent writes.

## Security

| Area | Practice |
|---|---|
| Threat model | Name assets, actors, entry points and trust boundaries; walk STRIDE: spoofing, tampering, repudiation, information disclosure, denial of service, elevation of privilege |
| Input | Validate at every trust boundary; parse into typed values; parameterize queries; encode output for its context (HTML, shell, SQL) |
| Authentication vs authorization | Authentication proves who; authorization checks each object access on the server (broken object-level authorization is the top API risk) |
| Least privilege | Minimal scopes for tokens, services and database roles; separate read and write credentials |
| Secrets | Keep them out of code, logs and URLs; rotate; load from a secret store |
| Passwords | Hash with Argon2id, scrypt or bcrypt, never a bare fast hash such as SHA-256 |
| Cryptography | Use vetted libraries and TLS 1.2+ (prefer 1.3); AEAD such as AES-GCM or ChaCha20-Poly1305; never reuse a nonce with the same key; compare secrets in constant time; draw randomness from a CSPRNG |
| Integrity | CRC catches accidents, a hash catches changes only against a trusted digest, an HMAC or signature proves origin |
| Server-side requests and files | Guard against SSRF, path traversal and deserialization of untrusted data |
| Supply chain | Lockfiles, pinned versions, vulnerability scanning, minimal dependencies |
| Audit | Log security events with actor and outcome, never with secrets |
| Zero trust | Authenticate and authorize every request; network location grants nothing |
| Service identity | mTLS between services; workload identity (short-lived, platform-issued credentials) instead of static keys |
| Encryption at rest | Envelope encryption: a data key encrypts the data, a key-encryption key in a KMS encrypts the data key; rotate keys on a schedule |
| Data | Classify by sensitivity; enforce data residency; privacy by design (collect less, keep it shorter) |
| Threat analysis | Attack trees for one goal, STRIDE per trust boundary, then mitigations with owners |
| Build provenance | SBOM per artifact, signed artifacts and provenance (SLSA levels), hermetic and reproducible builds |

| Access model | Decides by | Fits |
|---|---|---|
| RBAC | Roles granted to users | Stable job functions |
| ABAC | Attributes of user, resource and environment | Context rules: region, time, classification |
| ReBAC | Relationships in a graph (owner, member, shared with) | Documents, folders, social and multi-tenant sharing |
| PBAC | Central policies evaluated by a policy engine | Many services sharing one rule set |

A policy enforcement point (PEP) in each service asks a policy decision point (PDP) and enforces the answer. OAuth 2.0 delegates authorization; OpenID Connect adds identity on top. OWASP API Security Top 10 (2023) lists broken object-level authorization (BOLA) first and broken function-level authorization (BFLA) fifth: check each object and each operation on the server.

## Testing and verification

Test types, doubles and the flaky-test rule are in [testing concepts](concepts.md#testing-concepts). Pick by the defect you fear: property-based tests state invariants (round-trip, idempotence, ordering) and let the tool search; load and soak tests need production-like data and traffic shape. Mock only external boundaries.

## Observability and SRE

- **RED** for request-driven services: rate, errors, duration. **USE** for resources: utilization, saturation, errors. The four golden signals are latency, traffic, errors and saturation.
- **SLI, SLO and error budget.** Measure what users feel (successful requests within a latency bound), set a target, and spend the remaining budget on change.
- **Percentiles.** Report p50, p95, p99 and max from histograms or sketches; never average percentiles across hosts.
- **Logs.** Structured fields, a correlation or trace ID, no secrets or personal data.
- **Traces.** Propagate context across calls (W3C Trace Context) to see where latency goes.
- **Cardinality.** Every distinct label value is a time series: keep user IDs and URLs out of metric labels.
- **SLA.** The contractual promise with a penalty; set it looser than the internal SLO.
- **Burn rate.** Observed error rate / (1 − SLO). A burn rate of 1 spends the budget exactly over the window; 14.4 for one hour spends 2% of a 30-day budget, a common page threshold.
- **RTO and RPO.** How long recovery may take, and how much recent data may be lost; they set backup frequency and failover design.
- **MTTR and MTBF.** Mean time to restore and between failures; see the availability formula in [capacity](#capacity-and-estimation).
- **Incidents.** Classify severity by user and business impact; one incident commander coordinates; a blameless postmortem ends with owned actions; game days rehearse failures on purpose.
- **Telemetry semantics.** OpenTelemetry semantic conventions name attributes the same way across traces, metrics, logs and resources; sample traces and budget cardinality.

## Change and delivery

- **Expand–contract.** Add the new schema or field, write both, backfill, switch reads, then remove the old one in a later release.
- **Deploy order.** Make each step compatible with the version before and after it, so a rollback stays safe.
- **Progressive rollout.** Feature flags, canaries and blue-green deploys, with automatic rollback on SLO regression.
- **Reversibility.** Prefer changes you can undo; plan data migrations that cannot be undone as separate, rehearsed steps with a backup and restore test.
- **Batch size.** Small, frequent changes are easier to review, bisect and revert.

| Technique | Purpose | Watch |
|---|---|---|
| Infrastructure as code, GitOps | Version infrastructure; reconcile deployed state against Git | Detect and alert on drift instead of hand-fixing it |
| Immutable infrastructure | Replace instances instead of modifying them | Keep state outside the instance |
| Rolling, blue-green, canary | Replace instances gradually, switch whole environments, or expose a fraction of traffic | Rolling and canary run old and new together, so both must be compatible |
| Shadow traffic | Replay real traffic to the new version without user-visible effects | Suppress writes and external side effects |
| Deployment rings, progressive delivery | Roll out to ordered groups, gated by metrics | Automated rollback on the gate's signal |
| Environment parity, ephemeral environments | Keep stages alike; create short-lived isolated test systems | Same artifact promoted through every stage |
| Policy as code | Enforce deployment and infrastructure rules in the pipeline | Version and test the policies |
| Artifact provenance, reproducible builds | Prove what was built from which inputs; rebuild identical bytes from pinned inputs | Pin toolchains and dependencies; record digests |

## Code design

| Principle | Meaning |
|---|---|
| High cohesion, low coupling | Things that change together live together; modules depend on narrow interfaces |
| Dependency direction | Core logic does not depend on frameworks, I/O or delivery details |
| Single source of truth | One owner for each fact, setting and rule; derive the rest |
| Make illegal states unrepresentable | Encode invariants in types instead of comments and checks |
| Parse, don't validate | Convert raw input once at the boundary into types that carry the guarantee |
| Functional core, imperative shell | Pure decisions inside, side effects at the edges |
| Fail loudly at boundaries | Surface errors with context; never swallow them or return success-shaped fallbacks |
| Simplicity first | Solve today's general case; abstract when repeated real requirements justify it |

Module boundaries, cycles and refactors: [concepts](concepts.md), [analysis](analysis.md) and [change discipline](change-discipline.md).

## Debugging method

1. **Reproduce** reliably, with the exact input, version and environment.
2. **Read** the whole error and the first failure, not the last.
3. **Minimize** the input or the code until the failure is small.
4. **Bisect** in history (`git bisect`), data or code paths.
5. **Hypothesize and test** one variable at a time; write down what each result rules out.
6. **Check assumptions** about versions, configuration, environment and which binary actually runs.
7. **Fix the cause** with a regression test that failed first, then look for the same bug elsewhere.

Tools and bug classes: [debugging and observability concepts](concepts.md#debugging-and-observability-concepts) and [bug categories](concepts.md#bug-categories). To run the investigation on a live program (debugger, profiler, trace, dump), use `octocode-debugging` when installed.
