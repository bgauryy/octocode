<browser_agent>
**Browser agent** (`browser-agent` skill + `chromeDebug` + `spawnSubagent`):

Use `chromeDebug` directly for single-shot tasks (one screenshot, one network pass, one DOM query).
Use `spawnSubagent({agent:"browser-agent"})` for multi-turn browser sessions — the subagent stays alive between `AgentMessage` calls.

```
// Spawn once
agentId = spawnSubagent({agent:"browser-agent", task:"<phase 1>", url:"https://...", port:9222})
AgentMessage({action:"wait", agentId, timeoutMs:60000})

// Steer for follow-up phases
AgentMessage({action:"send", agentId, message:"now check cookies and storage"})
AgentMessage({action:"wait", agentId, timeoutMs:30000})

// Always kill when done
AgentMessage({action:"kill", agentId, remove:true})
```

Output protocol — parse these prefixes from `lastOutput`:
- `[FINDING]` — issue found; relay to user
- `[ACTION]` — next step recommendation
- `[BLOCKED]` — needs input; send answer via `AgentMessage(send)`
- `[DONE]` — phase complete; send next instruction or kill

**Multi-turn discipline:** give the subagent one clear phase per turn. It emits `[DONE]` when the phase is complete and waits. Do NOT give it 10 steps at once — that bypasses the multi-turn architecture.

**Kill discipline:** always `AgentMessage({action:"kill", agentId, remove:true})` after the last [DONE]. Agents do not self-terminate.

**Parallel browsers:** use different `port` values (9222, 9223…) — each gets its own Chrome profile automatically.
</browser_agent>
