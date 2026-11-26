Load when designing change data capture, data contracts, schema evolution, lineage, retention, event-time processing or other data-management practice.

# Data systems: data architecture and governance

## Data architecture and governance

| Concept | Role | Watch |
|---|---|---|
| OLTP / OLAP separation | Keep transactional and analytical workloads apart | Analytical scans on the primary steal its I/O and locks |
| Change data capture (CDC) | Stream committed changes from the database log | Per-key ordering, the snapshot-to-log handoff, schema changes in the stream |
| Data contracts | Producer-consumer expectations: schema, semantics, freshness, owner | Enforce in the producer's build, not by consumer complaints |
| Data catalog, lineage, provenance | Find data assets; trace origins and transformations; prove where data came from | Lineage drives impact analysis and deletion propagation |
| Data quality rules | Validate completeness, consistency and accuracy | Run at ingestion and quarantine failures instead of dropping them silently |
| Schema evolution | Change schemas safely | Backward- or forward-compatible by rule; see [API contracts](systems-api-contracts.md#api-and-interface-contracts) |
| Retention policies | Govern storage and deletion | Deletion must reach replicas, caches, backups and derived data; legal holds override it |
| Bitemporal data | Track valid time (when true in the world) and transaction time (when recorded) | Corrections without losing what was known when |
| Slowly changing dimensions | Preserve analytical history (type 1 overwrites, type 2 adds a row per version) | Type 2 needs effective dates and a current flag |
| Event-time processing, watermarks | Process by when events happened; a watermark asserts that earlier event times are complete | Too early drops late events, too late adds latency |
| Late event handling | Accept, update or side-output delayed input | Set allowed lateness explicitly |
| Reprocessing and replay | Rebuild derived state from the log | Needs retention, deterministic transforms and idempotent sinks |
| Reconciliation | Compare data between systems | Schedule it; it catches what every other check missed |

For schema and index choices, use [data structures](data-structures.md); for access control, use [security](systems-security.md#security); for telemetry and rollout, [observability, SRE and delivery](systems-operations.md). Benchmark with realistic skew, hot keys, cardinality and concurrency; verify the affected invariants before accepting a gain.
