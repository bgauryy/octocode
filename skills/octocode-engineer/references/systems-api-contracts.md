Load when designing or evolving an API or interface contract: versioning, pagination, errors, compatibility.

# Systems: API and interface contracts

## API and interface contracts

- **Specify the contract.** Units, ranges, nullability, ordering, limits, error codes, idempotency, consistency (read-your-writes or eventual) and thread safety.
- **Evolve additively.** Add optional fields and new endpoints; never change a field's meaning or type in place. Version when a breaking change is unavoidable and run both versions until callers move.
- **Paginate with cursors.** Offset pagination scans the skipped rows and shifts under inserts and deletes; an opaque cursor over a stable, unique sort key does neither. Add a tie-breaker such as the id: equal keys skip or repeat rows.
- **Respect HTTP method semantics.** Retry POST and PATCH only with an idempotency key; the safe and idempotent methods are listed under [HTTP semantics](network-http.md#http-semantics).
- **Return actionable errors.** A stable machine-readable code, a human message, the failing field, and whether a retry can succeed. Problem Details (RFC 9457, `application/problem+json`) gives type, title, status, detail and instance.
- **Bound inputs and outputs.** Maximum sizes, batch limits and timeouts are part of the contract, not implementation details.
- **Define the contract first.** API-first or schema-first: OpenAPI for HTTP, AsyncAPI for events, Protocol Buffers, Avro or JSON Schema for payloads, a schema registry that rejects incompatible event schemas.
- **Test compatibility.** Consumer-driven contract tests in the provider's build; publish a deprecation policy with dates and usage telemetry before removing anything.
- **Upcast old data.** Convert stored events or documents of an older shape on read, so readers handle one shape.
- **Negotiate capabilities.** Let client and server agree on protocol version and features instead of assuming them.
- **Use conditional requests.** An ETag saves a transfer or rejects a lost update ([HTTP semantics](network-http.md#http-semantics)).
- **Deduplicate requests.** Accept an [idempotency key](data-systems-distributed.md#distributed-data-and-consistency) on each mutation a caller may retry.
