# Cross-app agent contracts

Load when the same capability, handoff, task, message, artifact, or schema crosses agent apps, hosts, vendors, transports, or protocol versions. Why: matching field shapes do not prove matching meaning, and a universal envelope usually erases native lifecycle and authorization rules.

**Normalize semantics; adapt wire formats.** Keep one application-owned semantic contract and one adapter per external protocol or host. Do not rename native fields merely to make different protocols look alike.

`references/agents/agent-communication.md` owns interaction choice, authority, packet intent, and lifecycle. This reference owns cross-app projection, compatibility, and removal; it does not invent another canonical packet.

## Alignment map

Trace `semantic owner → producer → adapter/serialization → trust boundary → runtime validation/authorization → consumer → result/error`. Start from the chosen native interaction and record only material rows:

| Concern | Canonical decision | Adapter obligation |
|---|---|---|
| Identity and tenancy | actor, app/agent, intended receiver, tenant | map to native identity/auth fields; never trust model-supplied identity |
| Capability and authority | operation plus allowed scope and approval owner | distinguish discovery from authorization; reauthorize at the effect boundary |
| Correlation | request/message/task IDs and parent relationship | preserve opaque native IDs; do not reuse one ID for distinct messages |
| Payload | goal/input, media type, bounds, and expected result | use the target's supported schema dialect/subset; reject lossy conversion |
| Lifecycle | accepted, active, input-needed, terminal, cancelled/expired semantics | map native states explicitly; preserve retry, timeout, cancellation, and late-result behavior |
| Delivery and effects | side-effect class, idempotency identity, duplicate/replay policy | preserve delivery guarantees and deduplicate before repeating an effect |
| Result and error | artifact/data versus status/message; partial/completeness state; stable error meaning | retain typed results, actionable errors, and recovery handles without hiding them in prose |
| Version and extension | semantic-contract version and compatibility policy | negotiate at the protocol-owned location; reject unsupported required features instead of silent downgrade |

MCP models calls to tools; A2A models collaboration with independent agents; host SDK handoffs may transfer control inside one application. Choose the native interaction first, then map only the shared semantics. A schema validates shape at the boundary where it runs; it does not prove capability, authorization, relevance, delivery, or semantic equivalence.

## Schema rules

- Declare the schema dialect/version when the wire permits it. Compile or project the canonical model into each target's supported subset; do not assume full JSON Schema support or copy provider-only keywords across adapters.
- Use discriminated unions for mutually exclusive states. Define absent, `null`, empty, defaulted, and unknown-field behavior deliberately.
- Give every field one semantic name, unit, encoding, cardinality, sensitivity, and owner. Reuse a definition only when those meanings match; same shape is insufficient.
- Validate both ingress and egress. Keep model-generated routing metadata separate from trusted application state, credentials, and authorization context.
- Bound inline content and use artifact/resource references for large payloads. Specify retention, expiry, dereference authorization, and recovery for every handle.

## Change, merge, and removal gates

1. Inventory all producers, consumers, persisted payloads, generated clients, deployed protocol versions, caches, queues, and external integrations.
2. Classify the change as additive compatible, behavioral, or breaking for each consumer. A default change can be breaking without changing shape.
3. Add an adapter or version branch when native contracts differ. Do not create a second canonical schema or a permanent alias to avoid migration work.
4. Exercise valid, invalid, denied, partial, retry/replay, timeout/cancellation, and version-skew cases through the real producer and consumer seams.
5. Remove an alias, field, schema, adapter, or version only after no supported producer emits it, no supported consumer or stored payload needs it, the migration/retention window has closed, and rollback evidence exists. Empty repository references alone do not prove external absence.

Prefer removal when a field changes no reader action, duplicates an owned definition, or preserves an unsupported version with no consumer. Keep separate contracts when meanings, trust levels, lifecycle, or compatibility obligations differ—even when their JSON shapes match.

## Review output

```markdown
| Semantic concern | Owner | App/protocol mapping | Validation/authorization boundary | Compatibility | Keep/align/adapt/remove |
|---|---|---|---|---|---|
```

## Sources

- A2A, [protocol specification](https://a2a-protocol.org/dev/specification/) — independent-agent discovery, version negotiation, messages, tasks, artifacts, and lifecycle.
- Model Context Protocol, [2026-07-28 tools](https://modelcontextprotocol.io/specification/2026-07-28/server/tools) — model-to-tool discovery, calls, results, errors, and explicit state handles.
- OpenAI Agents SDK, [handoffs](https://openai.github.io/openai-agents-js/guides/handoffs/) — typed handoff inputs, history filtering, ownership transfer, and effect-boundary authorization.
- Anthropic, [strict tool use](https://platform.claude.com/docs/en/agents-and-tools/tool-use/strict-tool-use) — provider-supported schema subsets and schema-constrained tool inputs.
- JSON Schema, [Draft 2020-12](https://json-schema.org/draft/2020-12) — dialect identification, validation vocabularies, composition, and reusable schemas.

Next: choose the interaction with `references/agents/agent-communication.md`; implement a local Zod packet with `references/agents/zod-agent-contracts.md`; design tool-facing fields with `references/tools/tool-contracts.md`; audit tool-set drift with `references/tools/contract-audit.md`; verify negotiated MCP details with `references/tools/mcp-wire-contract.md`.
