# Cross-app agent contracts

Load when the same capability, handoff, task, message, artifact, or schema crosses agent apps, hosts, vendors, transports, or protocol versions. A universal envelope erases native lifecycle and authorization rules.

Normalize semantics; adapt wire formats. Keep one application-owned semantic contract and one adapter per external protocol or host. Do not rename native fields only to make protocols look alike. `agent-communication.md` owns interaction choice, authority, packet intent, and lifecycle; this page owns cross-app projection, compatibility, and removal, not another canonical packet.

## Alignment map

Trace `semantic owner → producer → adapter/serialization → trust boundary → runtime validation/authorization → consumer → result/error`. Start from the chosen native interaction; record only material rows.

| Concern | Canonical decision | Adapter obligation |
|---|---|---|
| Identity and tenancy | actor, app/agent, intended receiver, tenant | map to native identity/auth fields; never trust model-supplied identity |
| Capability and authority | operation, allowed scope, approval owner | distinguish discovery from authorization; reauthorize at the effect boundary |
| Correlation | request/message/task IDs, parent relationship | preserve opaque native IDs; never reuse one ID for distinct messages |
| Payload | goal/input, media type, bounds, expected result | use the target's schema dialect/subset; reject lossy conversion |
| Lifecycle | accepted, active, input-needed, terminal, cancelled/expired | map native states explicitly; keep retry, timeout, cancellation, late-result behavior |
| Delivery and effects | side-effect class, idempotency identity, replay policy | keep delivery guarantees; deduplicate before repeating an effect |
| Result and error | artifact/data vs status/message; completeness; stable error meaning | keep typed results, actionable errors, and recovery handles out of prose |
| Version and extension | semantic version, compatibility policy | negotiate at the protocol-owned location; reject unsupported required features, no silent downgrade |

MCP models tool calls; A2A models collaboration with independent agents; host SDK handoffs transfer control inside one app. Choose the native interaction first, then map only shared semantics. A schema validates shape where it runs; it does not prove capability, authorization, relevance, delivery, or semantic equivalence.

## Schema rules

- Declare the schema dialect/version when the wire permits it. Project the canonical model into each target's supported subset; do not copy provider-only keywords across adapters.
- Use discriminated unions for exclusive states. Define absent, `null`, empty, defaulted, and unknown-field behavior deliberately.
- Give each field one name, unit, encoding, cardinality, sensitivity, and owner. Reuse a definition only when meanings match; same shape is not enough.
- Validate ingress and egress. Keep model-generated routing metadata apart from trusted app state, credentials, and authorization context.
- Bound inline content; use artifact/resource references for large payloads, with retention, expiry, dereference authorization, and recovery for each handle.

## Change, merge, and removal gates

1. Inventory producers, consumers, persisted payloads, generated clients, deployed protocol versions, caches, queues, and external integrations.
2. Classify the change per consumer: additive, behavioral, or breaking. A default change can break without a shape change.
3. Add an adapter or version branch when native contracts differ; never a second canonical schema or a permanent alias to dodge migration.
4. Test valid, invalid, denied, partial, retry/replay, timeout/cancellation, and version-skew cases through the real producer and consumer seams.
5. Remove an alias, field, schema, adapter, or version only when no supported producer emits it, no consumer or stored payload needs it, the migration window closed, and rollback evidence exists. Empty repo references do not prove external absence.

Prefer removal when a field changes no reader action, duplicates an owned definition, or keeps an unsupported version with no consumer. Keep contracts separate when meaning, trust, lifecycle, or compatibility differ, even with matching JSON shapes.

Review output: `| Semantic concern | Owner | App/protocol mapping | Validation/authorization boundary | Compatibility | Keep/align/adapt/remove |`.

Sources: [A2A spec](https://a2a-protocol.org/dev/specification/), [MCP tools](https://modelcontextprotocol.io/specification/2026-07-28/server/tools), [OpenAI Agents SDK handoffs](https://openai.github.io/openai-agents-js/guides/handoffs/), [Anthropic strict tool use](https://platform.claude.com/docs/en/agents-and-tools/tool-use/strict-tool-use), [JSON Schema 2020-12](https://json-schema.org/draft/2020-12).

Next: Zod packet `zod-agent-contracts.md`; tool-facing fields `../tools/tool-contracts.md`; tool-set drift `../tools/contract-audit.md`; MCP details `../tools/mcp-wire-contract.md`.
