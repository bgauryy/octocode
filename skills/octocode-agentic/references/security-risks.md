Load when listing the risks of an agent design and their mitigations, connecting MCP servers, or securing traffic between agents.

# Security risks and agent-to-agent traffic

## Risks and mitigations

| Risk | Mitigation |
|---|---|
| Prompt injection, tool-output injection, goal hijack (LLM01, ASI01) | See [untrusted content](security.md#untrusted-content), and [limit what one session combines](security.md#limit-what-one-session-combines) |
| Excessive agency | Grant the minimum required permissions |
| Data exfiltration | Enforce destination and data-access controls |
| Credential exposure | Keep secrets outside model-visible context |
| Cross-tenant leakage | Tenant-scoped retrieval and caches |
| Confused deputy (ASI03) | Bind authorization to the requesting principal. An MCP proxy gets consent per client; a server accepts only tokens issued for it (audience check) and never passes a client's token through |
| Stolen handle or session ID | Possession is not authentication: bind each handle to the verified user and recheck on every use |
| Unsafe code execution (ASI05) | [Sandbox](security.md#sandbox-and-approval) filesystem and network, resource limits, approval |
| Malicious skill or tool, supply chain (LLM04, ASI04) | Validate origin, contents and executable resources. Pin tool descriptions (store a hash of each approved tool's name, description and schema, compare it on every listing, and block a changed tool until it is re-approved); watch for one server's description steering another server's tool |
| Unvalidated model output (LLM10) | Validate and encode output before a shell, query, browser or another agent consumes it |
| Cascading failure, rogue agent (ASI08, ASI10) | Bound delegation depth, cost and blast radius; stop and isolate an agent that leaves its task |
| Tool-argument manipulation | Server-side schema validation and authorization |
| Destructive or irreversible action | Approval at the irreversible step, shown with its real effect (ASI09: people over-trust agent summaries); test: the same call without approval is refused |
| Replay attack | Nonces, idempotency and authentication |
| Memory poisoning | Validate before writing durable memory |

Tool annotations, tool error hints and model-written call reasons are suggestions and audit records, not authorization: enforce permissions at execution time.

## Between agents

- Authenticate both callers and remote agent services; authorize every action at the receiving boundary.
- Encrypt transport; validate structured input before processing.
- Verify callback and webhook authenticity.
- Keep transport identity, user identity and delegated authority separate; a `sender` field is not identity.
- Do not route to an external agent because its description claims a capability.

Identifiers: OWASP Top 10 for LLM applications 2026 (`LLM`) and for agentic applications 2026 (`ASI`). Sources: [OWASP LLM Top 10](https://genai.owasp.org/llm-top-10/), [OWASP agentic Top 10](https://genai.owasp.org/resource/owasp-top-10-for-agentic-applications-for-2026), [MCP security best practices](https://modelcontextprotocol.io/docs/2026-07-28/tutorials/security/security_best_practices), [Invariant Labs: tool poisoning](https://invariantlabs.ai/blog/mcp-security-notification-tool-poisoning-attacks).
