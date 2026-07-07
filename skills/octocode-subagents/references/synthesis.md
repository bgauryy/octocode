# Synthesis

Use this before trusting worker output or finishing with spawned agents.

## Verification Rules

- Treat worker output as claims, not facts.
- Verify file and line evidence with local tools before relaying it.
- Reject findings without reproducible evidence.
- Reconcile disagreement between workers; do not choose a convenient result.
- Run `AgentMessage({ action: "list" })` before final answers and confirm every worker is terminal.

## Tool Allowlists

Workers default to all tools except `spawnAgent` and `AgentMessage`. Restrict when possible:

```json
{ "tools": ["localSearchCode", "localGetFileContent", "ghGetFileContent", "bash"] }
```

When `resourceMode:"lean"`, workers have no Octocode tools by default. Use `resourceMode:"octocode"` or pass the needed tools explicitly.

## Cleanup

- Kill stale or wrong-direction workers you no longer need.
- Use `remove:true` with `kill` or `wait` when the registry entry is no longer useful.
- Octocode kills spawned workers on `session_shutdown`, but mid-session cleanup still saves resources.
- `status` and `wait` truncate visible output; inspect details when the visible text is insufficient.
