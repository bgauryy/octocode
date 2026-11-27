Load when designing tool errors or checking MCP wire behavior.

# Tool errors and MCP wire

## Actionable errors

An error should tell the agent whether to retry, change the request, inspect another resource or stop. State what failed and give the corrected call: a generic error → `path must be workspace-relative; retry with path: "src/a.ts"`.

- A misplaced field reports where it belongs, for example: `cursor is the whole input; send {"cursor":"…"} alone`.
- Drop errors that only follow from the first one, such as a row that "lacks" its own fields because it was never a row.

| Code | Agent response |
|---|---|
| `INVALID_ARGUMENT` | Correct parameters using the schema |
| `RESOURCE_NOT_FOUND` | Check the identifier or search alternatives |
| `AMBIGUOUS_MATCH` | Narrow the scope |
| `PERMISSION_DENIED` | Stop or request authorization |
| `RATE_LIMITED` | Respect retry-after and budget |
| `TIMEOUT` | Smaller scope or another query |
| `RESULT_TOO_LARGE` | Add filters, page or request an artifact |
| `STALE_CURSOR` | Restart from a consistent snapshot |
| `CONFLICT` | Refresh state before modifying |
| `DEPENDENCY_UNAVAILABLE` | Use another source only if it answers the same question; say which source answered |
| `INTERNAL_ERROR` | Retry once if the error's retryability part (below) says the same call may be repeated and the call has no side effect; otherwise stop and report |
| `PARTIAL_SUCCESS` | Keep completed work, address the rest |

Separate the **error** (what failed), **cause** (the observable condition), **hint** (a valid next step), **retryability** (is repeating the same call safe) and **evidence** (partial result or diagnostic artifact). Hints are suggestions, not authorization ([security](security-risks.md#risks-and-mitigations)): a tool must not override the agent's security or task constraints.

## MCP wire

- Follow the [MCP specification](https://modelcontextprotocol.io/specification/latest) revision in use with the peer (negotiated by `initialize`, or declared per request from 2026-07-28); never merge revision-specific fields (long-running calls, multi-round-trip input, header mirroring); keep the older branch while peers use it.
- Each tool has a unique case-sensitive `name` and object `inputSchema` even with no arguments; a declared `outputSchema` binds `structuredContent`. Also return that JSON serialized in a text block (a SHOULD, for older clients): some clients show the model only `content` and some both, so measure the duplicate in the host's projection.
- Follow the `tools/list` cursor; cache hints only where the revision defines them.
- Return input validation and business failures as tool results with `isError: true`, so the model can [correct the call](#actionable-errors); keep protocol errors for an unknown tool, a malformed request or a server fault. Revisions before 2025-11-25 listed invalid arguments as protocol errors and SDKs lag, so check the error the server actually serves.
- Revision 2026-07-28 is stateless: it removes the `initialize` handshake, every request carries its protocol version and client capabilities in `_meta`, and instructions and capabilities arrive on the `server/discover` result (server info in its `_meta`). `mcp-surface.mjs capture` probes `server/discover` first and falls back to `initialize`, so it captures both; the capture's `handshake` field says which.
- Annotations are [untrusted hints](security.md#untrusted-content), not authorization.
- Cross-call state uses [references](tool-results.md#reference-first-responses); no implicit state.
- Definition or order change: new catalog version, refreshed discovery, rerun set audit, recheck [frozen prefixes](handoffs.md#frozen-base-prompt); never hot-patch one worker's cached copy.

## Sources

- [Model Context Protocol specification](https://modelcontextprotocol.io/specification/latest): tools page and changelog
