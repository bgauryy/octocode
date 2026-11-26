Load when defining SLOs, alerts or telemetry, or planning deployment, rollout, migration or rollback.

# Systems: observability, SRE and delivery

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
- **MTTR and MTBF.** Mean time to restore and between failures; see the availability formula in [capacity](systems-capacity.md#capacity-and-estimation).
- **Incidents.** Classify severity by user and business impact; one incident commander coordinates; a blameless postmortem ends with owned actions; game days rehearse failures on purpose.
- **Telemetry semantics.** OpenTelemetry semantic conventions name attributes the same way across traces, metrics, logs and resources; sample traces.

## Change and delivery

- **Expand–contract.** Add the new schema or field, write both, backfill, switch reads, then remove the old one in a later release.
- **Deploy order.** Make each step compatible with the version before and after it, so a rollback stays safe.
- **Reversibility.** Prefer changes you can undo; plan data migrations that cannot be undone as separate, rehearsed steps with a backup and restore test.
- **Batch size.** Small, frequent changes are easier to review, bisect and revert.

| Technique | Purpose | Watch |
|---|---|---|
| Infrastructure as code, GitOps | Version infrastructure; reconcile deployed state against Git | Detect and alert on drift instead of hand-fixing it |
| Immutable infrastructure | Replace instances instead of modifying them | Keep state outside the instance |
| Rolling, blue-green, canary | Replace instances gradually, switch whole environments, or expose a fraction of traffic | Rolling and canary run old and new together, so both must be compatible |
| Shadow traffic | Replay real traffic to the new version without user-visible effects | Suppress writes and external side effects |
| Deployment rings, progressive delivery | Roll out to ordered groups or behind feature flags, gated by metrics | Automatic rollback on the gate's signal, such as an SLO regression |
| Environment parity, ephemeral environments | Keep stages alike; create short-lived isolated test systems | Same artifact promoted through every stage |
| Policy as code | Enforce deployment and infrastructure rules in the pipeline | Version and test the policies |
| Artifact provenance, reproducible builds | Prove what was built from which inputs; rebuild identical bytes from pinned inputs | Pin toolchains and dependencies; record digests |
