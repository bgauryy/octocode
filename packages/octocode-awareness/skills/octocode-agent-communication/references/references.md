# References

The audit trail records the sources used to create `octocode-agent-communication`.

## Local Sources

| Source | Why it mattered |
|---|---|
| `packages/octocode-awareness/src/agents.ts` | Existing agent identity registry in the shared awareness DB |
| `packages/octocode-awareness/src/db.ts` | Confirms `agents`, `signals`, and `signal_reads` tables share one SQLite store |
| `packages/octocode-awareness/src/notifications.ts` | Existing `agentSignal` implementation for publish/list/reply/ack/resolve |
| `packages/octocode-awareness/src/types.ts` | Agent identity and signal contracts |
| `packages/octocode-awareness/src/pi-hooks.ts` | Pi startup registration and hook delivery behavior |
| `packages/octocode-awareness/skills/octocode-awareness/references/data-model.md` | Canonical awareness DB model |
| `packages/octocode-awareness/skills/octocode-awareness/references/hooks.md` | Host-specific hook constraints |
| `.octocode/rfc/octocode-agent-communication/RFC.md` | Decision to build one focused communication skill under awareness |
| `.octocode/rfc/octocode-agent-communication/IMPLEMENTATION.md` | Package-owned skill implementation steps |
| `.octocode/rfc/octocode-agent-communication/KPI.md` | Success criteria and guardrails |

## External Sources

| Source | Why it mattered |
|---|---|
| `https://a2a-protocol.org/latest/specification/` | A2A task, message, security, and transport concepts |
| `https://a2a-protocol.org/latest/topics/agent-discovery/` | Agent Card and discovery framing |
| `https://a2a-protocol.org/latest/topics/a2a-and-mcp/` | Boundary between A2A communication and MCP tools |
| `https://github.com/a2aproject/a2a-js` | Current JS SDK maturity signal |
