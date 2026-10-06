# Architecture

How the extension is laid out, for contributors. Users start with the [README](../README.md).

```bash
yarn workspace @octocodeai/pi-extension build      # tsc → dist/
yarn workspace @octocodeai/pi-extension test       # unit + end-to-end (real Pi session, scripted model, stub MCP server)
yarn workspace @octocodeai/pi-extension lint
pi --no-extensions -e packages/octocode-pi-extension/dist/index.js -e builtin:mcp -e builtin:tool-search
```

Source map: `src/index.ts` composes the features (each registers its own tools, hooks and commands), `src/turn.ts` sets each turn's active tools and prompt section, and `src/prompt.ts` is the prompt text; each folder is one domain and may depend only on the ones below it (enforced by `tests/architecture.test.ts`).

| Folder | Owns |
|---|---|
| `shared/` | Leaf helpers, one owner each: `util` (errors, caps, JSON parsing, content text, usage sums), `env` (flag and number parsing), `atomic` (crash-safe writes, SHA-256), `process` (liveness, process-tree kill), `home` (Octocode home and workspace paths, owner-only runtime folders, Pi's tool-path rule, repository root, the agent-state and per-session folders, output sweeping), `spill` (oversized results saved whole to the session's `output/` folder), `trust` (project-config trust), `package` (manifests, package root), `commands` (the `/octocode` subcommand router and `say`, command output that also reaches print and JSON modes; each subcommand holds the dialog lock), `locks` (the re-entrant per-tool lock, the one dialog queue every interaction holds so Pi never draws two dialogs at once, and the time a call spent in `tool_call` checks), `format`, `render` (shared tool presentation), `sanitize` (terminal escape and invisible-character stripping). |
| `files/` | The `file` tool (`tool`; its crash-safe writes are `shared/atomic`), edit checkpoints and `/octocode rewind` (`checkpoint`, with the on-disk journal and fork inheritance in `checkpoint-store`; retention is the sessions sweep), the optional change review (`review`), the `bash` override with its deadline and background jobs (`bash`), the bash guard (`bash-guard`: catastrophic commands), and how the `bash` and `file` calls and results are drawn (`render`). |
| `agentdb/` | The local agent database `octocode.db` (`db`: open, owner-only files, one shared connection per process, repository keys, the secret screen) and its versioned schema (v4) and forward migrations (`schema`: `sessions_v3`, `backlog`, `backlog_notes`, `memories`, `memories_fts`, `meta`, and the team tables `agents`, `messages`, `deliveries`, `leases`, `edits`). |
| `sessions/` | The session extras (`store`), its Pi wiring (`register`: extras rows, stats, resume brief), the `/sessions` picker (`command`), the resume brief (`brief`) and retention of session folders (`sweep`). |
| `backlog/` | The backlog store (`store`), the `backlog` tool (`tool`), `/octocode backlog` (`command`) and its TUI board (`board`), text and Markdown export (`format`), who is acting (`context`), and the status and widget wiring (`index`). |
| `memory/` | The memory store with FTS5 / `LIKE` search and duplicate checks (`store`, `query`), the `memory` tool (`tool`), automatic injection (`inject`, wired in `register`) and `/octocode memory` (`command`). |
| `ask/` | The `askUser` tool (`tool`) and its dialog (`dialog`). |
| `mcp/` | Registration of the built-in `octocode` server with Pi's MCP (`octocode`). |
| `web/` | The `web` tool (`web`) with URL fetching and caching (`fetch`), HTML-to-text (`html`), web search (`search`) and its private-address guard (`guard`). |
| `browser/` | The DevTools `browser` tool (`tool`, with its call and result rendering in `render`), the page snapshot and what changed since the last one (`snapshot`), mouse, keyboard, forms and waits (`input`), tabs and popups (`tabs`), dialog/upload/download (`actions`), per-page locale and download setup (`emulation`) and its protocol client (`cdp`, which also owns the abort error the browser files share). |
| `api/` | The external API (`bridge`): JSON-RPC 2.0 over a private Unix socket and optional loopback HTTP + SSE (`protocol`, `server`), instance discovery (`registry`), event replay (`events`), a client (`client`) and its command line (`cli`), and the Pi wiring with `/octocode api` (`register`). |
| `team/` | The team store on the agent database's team tables (`store`) and its record shapes (`model`), the recent-edits log (`edits`), this agent's own reservations and their idle lapse (`held`), routing and text (`routing`), this process's membership (`session`), paced delivery and acknowledgement retries (`inbox`), the `coordinate` and `sendMessage` tools (`tools`), the one shared 1-second ticker that beats delivery and refreshes the watchers' view (`ticker`), and the agents panel (`panel`). |
| `subagents/` | Profile loading (`profiles`), child process spawning and bounded event parsing (`process`), incremental report storage with a bounded preview (`report`), handoff folders and batched background-report delivery (`handoff`), the `agent` tool with background mode (`tool`), `/octocode agents` (`command`), how the `agent` call, its live progress and background reports are drawn (`render`), worktree isolation (`worktree`) and screenshot saving (`screenshots`). |
| `hooks/` | Optional Claude Code / Codex hooks: configuration (`config`), command runner (`runner`) and Pi event mapping (`register`). |
| `compaction/` | Tool-result trimming and `file` paths for Pi's summaries (`register`). |
| `ui/` | The banner header, spinner, thinking label and `tok/s` status (`chrome`, `banner`), the two-line footer (`footer`), the working line and team activity (`activity`), the queued-message ledger installed on `pi.sendMessage` that survives Esc (`delivery`), and the answer-ready / waiting-for-you terminal notifications with focus reports and the title marker (`notify`). |
| root | `prompt.ts` (the system prompt section), `turn.ts` (active tools and prompt per turn) and `skills.ts` (extra skill directories). |
