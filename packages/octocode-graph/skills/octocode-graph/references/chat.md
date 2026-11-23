Load when a person wants to ask the agent about graph nodes from the viewer, or the agent must answer viewer chat, point at nodes, or refresh the graph after editing code.

# Chat with the agent from the viewer

| Viewer behavior | Contract |
|---|---|
| Submit | **Ask the agent** (header button or `c`) attaches selected node context: path, line 1 and computed metrics; chip is removable. |
| Persist | Each message is a `chat` event in `events.jsonl`: `{type:'chat', id, role:'user'\|'agent'\|'system', text, context?, state?, replyTo?, at}`. Replay, including `--replay`, restores it. |
| Update | Same `id` updates `state`: `queued` → `delivered` → `replied`, or `failed` with `error`. |
| Render | Replies support code spans/fences; `path:line` mentions select matching graph nodes. |
| Point | Reply `nodes` are resolved IDs. Newest reply highlights and frames them; each retains **Show on graph (N)**. |

Reply with the message’s `chat_id`. Add `nodes` to highlight a file or cite `path:line` when a source location helps. The viewer can answer structural questions directly from the [saved graph](query.md). Community notes can supply context for architecture questions.

Only the token-authenticated browser socket (page token plus same-server `Origin`) can submit a message; that socket is the sender gate. `POST /reply`, `/refresh`, `/query` and `/wiki` need the token in `?t=` and accept no foreign `Origin`. `<out>/server.json` (`url`, `token`, `port`, `pid`, mode 600) exists while the server runs.

## Inbox mode (any session)

1. Start the viewer: `octocode-graph serve <section>` (or `--replay <result-dir>`); its stdout line names `out`.
2. Watch `<out>/chat-inbox.jsonl`; each message is one line `{chat_id, text, file, line, metrics?, at}`. In Claude Code, a Monitor on `tail -n 0 -F <out>/chat-inbox.jsonl` turns each line into a notification.
3. Answer with the reply command: `octocode-graph reply <out> "text" --chat <chat_id> [--nodes a.ts,src/b.ts]` (`-` reads the text from stdin; without `--chat` it answers the latest unanswered message; `--nodes` points at files on the graph). Ask the graph with `octocode-graph reply query <out> "impact a.ts"`; read or write the wiki with `octocode-graph reply wiki <out> read|note|promote|lint` ([query](query.md)).
4. To show edits in the viewer, run `octocode-graph refresh <out>`: the viewer serving it rebuilds the changed chunks and its page updates. Refreshes run one at a time; a request during a run queues one follow-up that later requests join.

`octocode-graph reply` prints one JSON line; exit 1 means the server refused, the refresh failed or a query failed (stderr says why; an ambiguous query or node lists candidates), 2 a usage error, 3 no running server for `<out>` (no or stale `server.json`, or unreachable).

## Channel mode (messages pushed into the session)

The graph channel is a Claude Code [channel](https://code.claude.com/docs/en/channels-reference.md), a research-preview feature: an MCP server over stdio that pushes each viewer message into the open session as `<channel source="graph-view" chat_id="…" file="…" line="…">text</channel>` and gives Claude these tools: `open_graph({ path, whole?, lsp?, result? })` (build and show a repository, or reopen a result directory; reuses an open viewer), `reply({ chat_id, text, nodes? })`, `query_graph({ query, path? })`, `wiki_read({ slug?, file? })`, `wiki_note({ slug, text })`, `wiki_promote({ slug?, chat_id?, text? })`, `wiki_lint()` and `refresh_graph({ path? })`.

Setup, done by the user:

1. Add to the project's `.mcp.json` (with the graph package installed):
   ```json
   { "mcpServers": { "graph-view": { "command": "octocode-graph", "args": ["channel"] } } }
   ```
2. Start the session with `claude --dangerously-load-development-channels server:graph-view`. Custom channels are not on Claude Code's channel allowlist during the preview; this flag bypasses the allowlist for that entry after a confirmation prompt, so running it is the user's own decision. An agent never runs it or starts a channel session itself.

Limits from the channel docs:

- Channels need claude.ai or Console authentication (not Amazon Bedrock, Google Cloud's Agent Platform or Microsoft Foundry); Team and Enterprise organizations must enable them.
- Messages arrive only while that session is open; a permission prompt pauses the session until someone answers it in the terminal.
- `octocode-graph channel` answers `initialize` with protocol `2025-06-18` (or the client's own earlier revision), because Claude Code does not register a channel that negotiates MCP revision 2026-07-28. If a client still lands on that revision, `MCP_PROTOCOL_NEGOTIATION=legacy` keeps every stdio server on the earlier handshake.
- `open_graph` resolves paths from the session's working directory and writes results under its `.octocode/octocode-graph/`; `GRAPH_VIEW_NO_OPEN=1` skips opening the browser.
