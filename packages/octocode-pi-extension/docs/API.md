# External API

Let other programs message a running Pi session and follow its events. Off by default. See the [README](../README.md) for an overview.

Off by default: whoever can reach the API can drive the agent with your permissions. Start Pi with `OCTOCODE_API=1` (socket) and/or `OCTOCODE_API_HTTP=0` (loopback HTTP), or type `/octocode api on`.

**Transports.** Socket: one JSON-RPC 2.0 message per line; the file permissions are the authentication. HTTP: `POST /rpc` (one request), `GET /events[?since=N&types=a,b]` (Server-Sent Events from the next event; `since` or `Last-Event-ID` replays the buffered ones after N), `GET /health`; bound to `127.0.0.1`, every request needs `Authorization: Bearer <token>` from the instance file, and requests with a browser `Origin` or a non-loopback `Host` are refused (no CORS, so a web page cannot drive the agent). Frames are capped at 1 MiB, 16 connections, and a client that stops reading is dropped.

| Method | Params | Result |
|---|---|---|
| `initialize` | – | protocol version, instance, capabilities |
| `status` | – | `state` (`idle`/`working`), model, cwd, session, context usage, latest event `seq` |
| `message.send` | `text` (≤ 32,000), `from`, `mode` (`auto`/`steer`/`followUp`), `wait`, `timeoutMs` | `{id, queued}`; with `wait: true` also the agent's final `text` and `stopReason`, returned once Pi has settled (after any queued follow-up or retry), not at the first `agent_end` (error `-32001` on timeout; the message is still delivered) |
| `turn.abort` | – | `{aborted}` |
| `messages.list` | `limit` (≤ 100) | recent user/assistant text |
| `events.subscribe` / `events.unsubscribe` | `since`, `types` | socket only: `event` notifications follow; `{seq, gap}` says whether `since` fell out of the 500-event buffer |
| `agents.list` / `agents.tell` | `to`, `text` | the team's agents; message one as the user |

`message.send` arrives in the transcript as "Message from `<from>` through the Octocode API" and starts a turn when the agent is idle; a busy agent reads it at its next step (`steer`) or after the current run (`followUp`). Text is stripped of terminal escape sequences like all untrusted text.

Events (`seq` strictly increasing per instance): `agent.start`, `agent.end` (`text`, `stopReason`), `agent.settled` (the final `text` once Pi will not continue), `message` (`role`, `text`), `tool.start` (`id`, `name`, `hint`), `tool.end` (`id`, `name`, `isError`, `durationMs`; no arguments or output), `external.message`, `compaction`, plus opt-in `message.delta` (streamed text, request it in `types`).

Both tool events carry `depth`: 0 for the agent's own calls. Calls a tool makes itself (for example from a codemode script) also carry `parentId`, the calling tool's id; Pi names them `<parentId>/<n>`, and provider ids contain no `/`, so `depth` is the number of `/`-separated segments in `parentId` (`toolu_1/1` has depth 1, `toolu_1/1/2` depth 2). `durationMs` is the time between the bridge's own `tool.start` and `tool.end` for that id; it is absent when the start was not observed (the API started mid-call). `octocode-pi-api watch --top-level` (or `ApiClient.watch({ topLevel: true })`) drops tool events that have `parentId`.

```bash
OCTOCODE_API=1 pi                                # in one terminal
octocode-pi-api list                             # discover instances
octocode-pi-api send --wait "run the tests and report failures"
octocode-pi-api watch --types agent.end,tool.start --top-level
```

From code, `ApiClient` in `src/api/client.ts` is the reference client (`call`, `watch`).
