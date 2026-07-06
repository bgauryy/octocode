---
name: browser-agent
description: "Use when browser work needs multiple turns of Chrome DevTools Protocol interaction: security/cookie/storage audits, network analysis, DOM inspection, coverage, workers/service-workers, device emulation, or multi-step automation. Spawns a dedicated browser subagent via spawnSubagent that stays alive for follow-up instructions via AgentMessage. For single-shot tasks (one screenshot, one network pass), call chromeDebug directly instead."
---

# Browser Agent

Spawn a dedicated Chrome DevTools Protocol subagent for multi-turn browser work.
The subagent has `chromeDebug` + `web` + local read tools and emits structured output.

## Single-shot vs multi-turn

| Use `chromeDebug` directly | Use `spawnSubagent(browser-agent)` |
|---|---|
| One screenshot | Security + storage + network audit in sequence |
| One-pass network log | Watch network while user interacts |
| Quick console check | Iterative debugging with follow-ups |
| Single DOM query | Coverage → interact → re-measure |
| Any single scheme call | Any task needing 2+ separate CDP operations |

## Spawn

```
spawnSubagent({
  agent: "browser-agent",
  task:  "<what to do — be specific>",
  url:   "https://example.com",    // optional: target URL
  port:  9222,                     // optional: Chrome debug port (default 9222)
  launch: false,                   // optional: start Chrome if not running
})
→ { agentId: "abc123…" }
```

The subagent receives the pre-built system prompt (CDP reference + chromeDebug guide + protocol).
It stays alive and waits for follow-up instructions via AgentMessage.

## Multi-turn coordination

```
// Spawn
agentId = spawnSubagent({agent:"browser-agent", task:"audit https://example.com security", url:"https://example.com"})

// Wait for first pass
AgentMessage({action:"wait", agentId, timeoutMs:60000})

// Steer (interrupt current turn) or send (queue after current turn)
AgentMessage({action:"send", agentId, message:"now check the /api/login endpoint too"})
AgentMessage({action:"wait", agentId, timeoutMs:30000})

// Done — collect and kill
AgentMessage({action:"status", agentId})   // read full output
AgentMessage({action:"kill",   agentId, remove:true})
```

## Output protocol

The subagent prefixes every line:

| Prefix | Meaning |
|---|---|
| `[STATUS] …` | Progress — what it's doing |
| `[FINDING] …` | Issue or discovery with specifics |
| `[ACTION] …` | Recommended next step |
| `[METRIC] …` | Measurement (size, count, %, ms) |
| `[SCREENSHOT] path` | Absolute path to screenshot |
| `[BLOCKED] reason` | Needs input before continuing |
| `[DONE] summary` | Task complete |

Parse `AgentMessage(status).lastOutput` for these prefixes.
Relay `[FINDING]` and `[ACTION]` lines to the user.
Pass `[BLOCKED]` reason back via `AgentMessage(send, message: answer)`.

## Async polling (long tasks)

For tasks that take > 30s, poll instead of blocking:
```
agentId = spawnSubagent({agent:"browser-agent", task:"run 30s monitor", url:"...", port:9222})
// Poll every 10s while working on something else
while True:
  status = AgentMessage({action:"status", agentId})
  if status.status == "idle":  // [DONE] emitted, waiting
    break
  // optionally: print status.lastOutput preview
  wait 10s
AgentMessage({action:"kill", agentId, remove:true})
```

## Kill discipline (always)

**Always kill the agent after the last [DONE].** Agents do not self-terminate.
```
AgentMessage({action:"kill", agentId, remove:true})
```
If the agent is stuck > 2× expected time:
```
AgentMessage({action:"abort", agentId})  // graceful interrupt
// wait 5s, then send next instruction or kill
AgentMessage({action:"kill", agentId, remove:true})
```

## Parallel browsers

Spawn multiple simultaneously for independent audits:
```
secId = spawnSubagent({agent:"browser-agent", task:"security audit",     url:"https://example.com"})
perfId = spawnSubagent({agent:"browser-agent", task:"performance audit", url:"https://example.com", port:9223})
AgentMessage({action:"wait", agentId:secId,  timeoutMs:90000})
AgentMessage({action:"wait", agentId:perfId, timeoutMs:90000})
```

## chromeDebug scheme quick reference

The subagent uses these schemes internally — you can also request them explicitly:

| Scheme | What it covers |
|---|---|
| `debug` | Exceptions + HTTP errors + blocked + DOM state + screenshot |
| `network` | Requests/responses + cookie flags |
| `security` | CSP/HSTS/X-Frame + cookie flags + localStorage sensitive keys |
| `storage` | Cookies + localStorage + sessionStorage + IndexedDB + Cache + quota |
| `accessibility` | AX tree: unlabeled elements, missing alt, heading levels |
| `workers` | Web workers + service workers (lifecycle + scriptURL) |
| `performance` | Core Web Vitals, JS heap, layout counts |
| `coverage` | CSS rule usage + JS function coverage |
| `emulate` | Device viewport, network throttle, geolocation |
| `intercept` | Request capture/mock (Fetch domain) |
| `screenshot` | PNG/JPEG/PDF capture |
| `raw` | Any `Domain.Method` — full CDP access |

## Terminal visibility — see what the agent is doing

**Option 1 — CDP event log** (raw CDP traffic):
```bash
# Enable before spawning:
OCTOCODE_CDP_DEBUG=1 pi ...

# Tail in another terminal:
tail -f ~/.octocode/chrome-debug/port-9222/cdp-events.jsonl
# Pretty-print:
tail -f ~/.octocode/chrome-debug/port-9222/cdp-events.jsonl | python3 -c "import sys,json; [print(json.dumps(json.loads(l), indent=None)) for l in sys.stdin]"
```

**Option 2 — Pi TUI** shows every chromeDebug tool call the subagent makes in real-time (tool name + params).

**Option 3 — poll subagent output**:
```
AgentMessage({action:"status", agentId})  // read lastOutput field (up to 12KB)
```
Call every 5–10s during long tasks to see [STATUS]/[FINDING] lines as they arrive.

**Option 4 — Chrome DevTools Protocol Monitor** (visible Chrome only):
Open DevTools → Settings → Experiments → “Protocol Monitor” → More Tools → Protocol Monitor.

## Error recovery

| Signal | What to send |
|---|---|
| `[BLOCKED] Chrome not running` | `AgentMessage(send: "use launch:true or start Chrome manually")` |
| `[BLOCKED] auth required` | Tell user to log in, then `AgentMessage(send: "continue")` |
| Agent `failed` status | `AgentMessage(status)` → read error → `kill` → re-spawn with fix |
| Agent stuck > 2× expected time | `AgentMessage(abort)` → wait 5s → `AgentMessage(send, new instruction)` |

## Reference

- `references/CDP_QUICK_REF.md` — all 57 CDP domains with key methods/events
