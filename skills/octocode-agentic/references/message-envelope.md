Load when choosing message metadata or writing contract tests for agent communication.

# Message envelope and contract tests

## Message envelope

Use the native protocol's envelope. Add application metadata only for a concrete consumer; the examples below describe meanings, not a required field set:

| Field | Purpose |
|---|---|
| `messageId` | Unique message identity |
| `taskId` | Logical task identity |
| `correlationId` / `causationId` | Related messages / the event that caused this one |
| `sender` / `recipient` | Routing and audit |
| `kind` | Command, event, query, result, error or cancel |
| `schemaVersion` | Envelope version |
| `payloadSchema` | Selects the versioned payload validator, applied after the envelope |
| `deadlineAt` | Bounds useful execution time |
| `idempotencyKey` | Deduplicates logical operations |

Production envelopes may add trace context, tenant scope, content type, priority and artifact references. Replies mint a new `messageId` and use `causationId` for the replied-to message (the `inReplyTo` relationship), preserving task correlation. Identity and authorization come from authenticated transport or verified credentials, never from the `sender` field.

## Testing

| Test | Validates |
|---|---|
| Contract | Message conforms to its schema |
| Consumer-driven contract | Producer meets consumer assumptions |
| Compatibility | Old and new versions interoperate |
| Property-based | Invariants across generated inputs |
| Fuzz | Malformed payload handling |
| Duplicate delivery | Idempotency |
| Reordering | Out-of-order behavior |
| Crash / restart | Durable recovery |
| Timeout | Deadline behavior |
| Cancellation | Cancellation and side effects |
| Authorization | Cross-agent permission enforcement |
| Streaming | Update order and artifact assembly |
| Load | Throughput, queueing, saturation |
| Chaos | Partial failures and partitions |

Test real contract fixtures, not only compiled types ([typed messages](typed-messages.md#typed-messages)).
