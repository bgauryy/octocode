Load when a person wants to ask the agent about graph nodes from the viewer, or the agent must answer viewer chat, point at nodes, or refresh the graph after editing code.

# Chat with the agent from the viewer

The viewer's **Ask the agent** drawer (header button or `c`) sends a message with the selected node attached as context (path, line 1, its metrics once computed; the chip is removable). Every message is a `chat` event in `events.jsonl` (`{type:'chat', id, role:'user'|'agent'|'system', text, context?, state?, replyTo?, at}`; a later event with the same `id` updates `state`: `queued` → `delivered` → `replied`, or `failed` with `error`), so a replay restores the conversation, also in `--replay`. Replies render `code` spans and fenced blocks, and `path:line` mentions of graph nodes become links that select the node. A reply may carry `nodes` (resolved ids): the newest lights them up and frames them, and each keeps a **Show on graph (N)** action.

Answer from the graph and the wiki, not memory: read the community's wiki page, prove structural claims with graph queries (impact, path, cycle, community, callers) and source or LSP reads, reply with the files as `nodes`, and promote an answer worth keeping. Queries, pointing and the wiki: [query](query.md).

Only the token-authenticated browser socket (page token plus same-server `Origin`) can submit a message; that socket is the sender gate. `POST /reply`, `/refresh`, `/query` and `/wiki` need the token in `?t=` and accept no foreign `Origin`. `<out>/server.json` (`url`, `token`, `port`, `pid`, mode 600) exists while the server runs.

## Inbox mode (any session)

1. Start the viewer: `node <skill>/scripts/serve.mjs <section>` (or `--replay <result-dir>`); its stdout line names `out`.
2. Watch `<out>/chat-inbox.jsonl`; each message is one line `{chat_id, text, file, line, metrics?, at}`. In Claude Code, a Monitor on `tail -n 0 -F <out>/chat-inbox.jsonl` turns each line into a notification.
3. Answer with [reply.mjs](../scripts/reply.mjs): `node <skill>/scripts/reply.mjs <out> "text" --chat <chat_id> [--nodes a.ts,src/b.ts]` (`-` reads the text from stdin; without `--chat` it answers the latest unanswered message; `--nodes` points at files on the graph). Ask the graph with `reply.mjs query <out> "impact a.ts"`; read or write the wiki with `reply.mjs wiki <out> read|note|promote|lint` ([query](query.md)).
4. After editing code: add `--refresh` to the reply, or run `node <skill>/scripts/reply.mjs <out> --refresh`. Refreshes run one at a time; a request during a run queues one follow-up that later requests join.

`reply.mjs` prints one JSON line; exit 1 means the server refused, the refresh failed or a query failed (stderr says why; an ambiguous query or node lists candidates), 2 a usage error, 3 no running server for `<out>` (no or stale `server.json`, or unreachable).

## Channel mode (messages pushed into the session)

[channel.mjs](../scripts/channel.mjs) is a Claude Code [channel](https://code.claude.com/docs/en/channels-reference.md), a research-preview feature: an MCP server over stdio that pushes each viewer message into the open session as `<channel source="graph-view" chat_id="…" file="…" line="…">text</channel>` and gives Claude these tools: `open_graph({ path, whole?, lsp?, result? })` (build and show a repository, or reopen a result directory; reuses an open viewer), `reply({ chat_id, text, nodes? })`, `query_graph({ query, path? })`, `wiki_read({ slug?, file? })`, `wiki_note({ slug, text })`, `wiki_promote({ slug?, chat_id?, text? })`, `wiki_lint()` and `refresh_graph({ path? })`. Its instructions ask Claude to read the wiki page, prove with `query_graph` and LSP, answer with `reply` and `nodes`, promote good answers, and refresh after edits.

Setup, done by the user:

1. Add to the project's `.mcp.json` (use the absolute skill path):
   ```json
   { "mcpServers": { "graph-view": { "command": "node", "args": ["<skill>/scripts/channel.mjs"] } } }
   ```
2. Start the session with `claude --dangerously-load-development-channels server:graph-view`. Custom channels are not on Claude Code's channel allowlist during the preview; this flag bypasses the allowlist for that entry after a confirmation prompt, so running it is the user's own decision. An agent never runs it or starts a channel session itself.

Limits from the channel docs:

- Channels need claude.ai or Console authentication (not Amazon Bedrock, Google Cloud's Agent Platform or Microsoft Foundry); Team and Enterprise organizations must enable them.
- Messages arrive only while that session is open; a permission prompt pauses the session until someone answers it in the terminal.
- `channel.mjs` answers `initialize` with protocol `2025-06-18` (or the client's own earlier revision), because Claude Code does not register a channel that negotiates MCP revision 2026-07-28. If a client still lands on that revision, `MCP_PROTOCOL_NEGOTIATION=legacy` keeps every stdio server on the earlier handshake.
- `open_graph` resolves paths from the session's working directory and writes results under its `.octocode/octocode-graph-view/`; `GRAPH_VIEW_NO_OPEN=1` skips opening the browser.

Owners: [serve.mjs](../scripts/serve.mjs) (`startServer`, chat state, `/reply`, `/refresh`), [serve-answers.mjs](../scripts/serve-answers.mjs) (`/query`, `/wiki`, reply nodes), [live-chat.js](../assets/live-chat.js) (drawer), [chat.test.mjs](../scripts/chat.test.mjs) (handshake, delivery, token gate, refresh queue, queries and pointing; hermetic).
