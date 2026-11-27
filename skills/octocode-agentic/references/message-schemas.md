Load when one message schema must serve several boundaries, providers or applications.

# Message schemas across boundaries

## One schema, many boundaries

```text
Canonical contract (Zod / JSON Schema / Protobuf)
  → LLM outputs (constrain structure) · tool inputs (validate parameters) · tool outputs (validate observed data)
  → agent handoffs (validate delegated results) · MCP schemas · API documentation
```

One canonical contract reduces drift, but each target accepts a different dialect: export per target as [schema](tool-descriptions.md#schema) describes, then validate the returned value and business invariants in application code.

## Schema rules

- Field and branch rules belong to [tools](tool-descriptions.md#schema). Declare the dialect where the wire allows.
- Keep model-generated routing metadata apart from trusted state and credentials.
- Bound strings and arrays according to the owning workload. Large payloads use [references](tool-results.md#reference-first-responses) with retention, expiry and recovery.
- Schemas check shape, not authority or meaning. Reject unknown fields between agents; extend only via a versioned extension field.

## Cross-app alignment

- One app-owned semantic contract, one adapter per protocol; never rename native fields to match or build a universal envelope.
- Same shape but different meaning, trust, lifecycle or compatibility → separate contracts.
- Audit a crossing along `owner → producer → adapter → trust boundary → validation/authorization → consumer → result/error`; material rows only; decide each concern once, enforce it in the adapter:
  - Identity, tenancy (actor, agent, receiver, tenant): map to native auth; never trust model-supplied identity.
  - Authority (operation, scope, approval owner): reauthorize at the effect; discovery is not authorization.
  - Correlation: native IDs opaque; one ID per message.
  - Payload (media type, bounds, expected result): target dialect; reject lossy conversion.
  - Lifecycle (accepted, active, input-needed, terminal, canceled): map states; keep retry, timeout, cancel and late-result behavior.
  - Effects (side-effect class, idempotency key): deduplicate before replaying an effect.
  - Result, error (artifact vs status, completeness): typed results, recovery handles.
  - Version: negotiate where defined; no silent downgrade.

## Sources

- [OpenAI: structured outputs](https://developers.openai.com/api/docs/guides/structured-outputs)
