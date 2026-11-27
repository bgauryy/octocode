Load when messages need delivery guarantees, retries, streaming or artifacts.

# Delivery, streaming and artifacts

## Reliability and delivery

| Concern | Approach |
|---|---|
| Duplicated messages | Stable idempotency keys |
| Lost messages | Durable broker or outbox where required |
| Out-of-order delivery | Sequence numbers and version checks |
| Repeated commands | Idempotent handlers |
| Agent crash | Durable checkpoints, resumable tasks |
| Agent timeout | Query task state before repeating side effects |
| Partial completion | Persist completed steps and remaining work |
| Concurrent modification | Optimistic locking or transactional serialization |
| Long-running work | Task IDs with polling, streaming or callbacks |
| Cancellation | Explicit, acknowledged cancellation |
| Backpressure | Bounded queues and admission control |
| Dead letters | Quarantine failed messages with diagnostics |
| Schema mismatch | Reject, negotiate or migrate versions |
| Duplicate results | Deduplicate by task and artifact identity |

Exactly-once messaging is not exactly-once business effects. Prefer at-least-once delivery with idempotent processing and durable deduplication.

## Streaming and artifacts

| Event | Content |
|---|---|
| `task.started` | Accepted, ID assigned |
| `task.progress` | Meaningful stage transition |
| `task.blocked` | Missing input or permission |
| `artifact.available` | Reference to a produced artifact |
| `task.completed` | Final result and verification status |
| `task.failed` | Error, retryability, recovery options |
| `task.canceled` | Confirmed cancellation |

Stream semantic progress, not internal reasoning text. Where updates can be lost, keep authoritative task state queryable. In A2A, artifacts can stream incrementally while task status stays queryable.
