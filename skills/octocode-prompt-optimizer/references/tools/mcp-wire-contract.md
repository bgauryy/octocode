# MCP tool wire contract

Load when optimizing MCP discovery, negotiated-version behavior, tool definitions, calls, results, caching, errors, or cross-call state. Branch on the negotiated protocol version; never merge revision-specific fields into one imaginary schema. Verify the live schema or official spec before editing.

## Current revision check

For MCP `2026-07-28`:

- Every request carries the required protocol/client metadata. `tools/list` is paginated, returns a typed `resultType`, and includes cache hints such as `ttlMs` and `cacheScope`; stable tool sets use deterministic ordering. A `listChanged` server notifies clients subscribed through `subscriptions/listen`.
- A tool definition has a unique case-sensitive `name`, optional display metadata, a valid `inputSchema` JSON Schema object, and optional `outputSchema` and `annotations`. JSON Schema 2020-12 composition is allowed; `outputSchema` may describe any JSON value, and `structuredContent` must conform when that schema exists. A no-argument tool still declares an object input schema.
- Streamable HTTP may mirror eligible primitive arguments with `x-mcp-header`; validate its constraints and never expose secrets or sensitive data through headers.
- `tools/call` validates `name` and `arguments`. Results can be `complete` or `input_required`; retries carry the requested inputs and any opaque `requestState` under a new JSON-RPC request ID.
- Separate protocol errors from actionable tool-execution errors. Validate structured results against `outputSchema`, preserve typed content and recovery state, and treat annotations as untrusted hints rather than authorization.
- MCP has no implicit cross-call session state. A server mints an opaque handle, returns it, accepts it on later calls, reauthorizes every use, and documents expiry/recovery.

## Compatibility gate

Keep the older branch when the peer negotiates it. For `2025-11-25`, `structuredContent` and `outputSchema` are object-rooted, tool discovery lacks the 2026 cache-result contract, and Tasks use the older experimental core vocabulary. For `2026-07-28`, Tasks are a separately negotiated extension and core tool calls can use multi round-trip `input_required` results. Do not upgrade a prompt by changing only field names; migrate server, client, tests, and cached tool catalog together.

When a definition or ordering changes, publish a new catalog version, refresh discovery, run `contract-audit.md`, and verify frozen agent prefixes with `../agents/agent-prompt-integrity.md`. Never hot-patch one worker's cached copy.

Sources: [MCP 2026-07-28 Tools](https://modelcontextprotocol.io/specification/2026-07-28/server/tools); [2026-07-28 release](https://blog.modelcontextprotocol.io/posts/2026-07-28/); [MCP Tasks extension](https://tasks.extensions.modelcontextprotocol.io/specification/draft/tasks).

Next: layer ownership `tool-contracts.md`; set audit `contract-audit.md`; result bounds `../context/context-budget.md`; catalog prefix `../context/prompt-caching.md`.
