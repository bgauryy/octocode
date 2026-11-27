Load when changing, versioning or removing a message or tool schema field.

# Schema versioning

| Change | Compatibility |
|---|---|
| Add optional field | Usually compatible for tolerant consumers |
| Add required field | May break existing producers |
| Rename or remove field | Breaking for consumers relying on it |
| Change field type | Usually breaking |
| Add enum value | Can break exhaustive consumers |
| Change meaning or units | Semantic break unless versioned |
| New message type | Needs capability negotiation or tolerant handling |

```text
contracts/
  agent-envelope/v1.schema.json
  architecture-findings/v1.schema.json
  architecture-findings/v2.schema.json
  task-result/v1.schema.json
```

Run compatibility checks in CI, keep fixtures for older producers and consumers, and use explicit upcasters when reading historic persisted events. A breaking shape bumps the version; accept old versions only in an explicit migration window.

## Change and removal

- Inventory producers, consumers, stored payloads, generated clients and deployed versions → classify per consumer as additive, behavioral or breaking (a default change can break without a shape change) → bridge with an adapter or version branch, never a second canonical schema or permanent alias → test valid, invalid, denied, partial, replay, timeout, cancel and version-skew cases through real seams.
- Remove only when nothing emits or needs the contract (stored payloads included), the migration window closed and rollback evidence exists; an empty repo search does not prove external absence.
- Report per concern: owner, mapping, validation boundary, compatibility and verdict (keep, align, adapt, remove).
