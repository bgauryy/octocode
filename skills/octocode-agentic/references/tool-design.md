Load when deciding which kind of capability a need calls for, or checking how an MCP server handles identity, repeats, cancels, timeouts and failures.

# Tool and MCP server design

## Capabilities and triggers

| Need | Choose |
|---|---|
| A fixed sequence known in advance | Workflow, not an agent loop |
| Runs on a lifecycle event without the model deciding | Hook, not a tool |
| A procedure plus references the model reads on demand | Agent Skill, not server instructions |
| A reusable prompt template the user or a workflow invokes | MCP prompt |
| Read-only context the client selects | MCP resource |
| An action, or a read the model chooses | Tool |
| A bounded independent subtask that needs its own context | Subagent ([delegation](delegation.md#delegate-only-separable-work)) |

## MCP server design

| Concern | Check |
|---|---|
| Authentication | Results are scoped to the caller's identity; test with a second identity |
| Idempotency | A repeated call with the same key has one effect; test by sending it twice ([delivery](message-delivery.md#reliability-and-delivery)) |
| Cancellation | A cancel stops the work and the reply says what already took effect |
| Timeouts | The tool returns a typed timeout before the host's deadline |
| Observability | A failed call leaves enough in its result or log to name the failing step |
