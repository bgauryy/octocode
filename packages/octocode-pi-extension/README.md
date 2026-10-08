# Octocode for Pi

Turn [Pi](https://pi.dev) into a research-driven coding agent. Octocode adds code research (local search, LSP, GitHub and package registries through [Octocode MCP](https://github.com/bgauryy/octocode)), subagents, a safer file tool, a per-repository backlog and memory, and a browser. It builds on Pi's own tools, MCP, sessions, compaction and UI instead of replacing them.

```bash
pi install npm:@octocodeai/pi-extension    # install for every session
pi -e npm:@octocodeai/pi-extension         # or try it once
```

Requires Pi 0.99.2 or later and Node.js 22.13 or later.

## Contents

- [Built on the Octocode ecosystem](#built-on-the-octocode-ecosystem)
- [Quick start](#quick-start)
- [How Octocode extends Pi](#how-octocode-extends-pi)
- [Features](#features)
- [Tools the agent gets](#tools-the-agent-gets)
- [Commands](#commands)
- [Best practices](#best-practices)
- [Configuration](#configuration)
- [Documentation](#documentation)
- [Development](#development)

## Built on the Octocode ecosystem

Octocode for Pi is the Pi front end of the [Octocode](https://github.com/bgauryy/octocode) toolchain. The same research engine and skills also run in Claude Code, Codex and the `octocode` CLI, and Octocode keeps its config and state in one `.octocode` layout, so what you set up once carries across agents.

```
            Pi (tools, MCP, sessions, compaction, UI)
                          │
            Octocode for Pi extension
   ┌──────────────┼───────────────────┐
Octocode MCP     Octocode skills     .octocode home + workspace
(research)       (workflows)         (config and agent state)
```

| Part | What it is | How the extension uses it |
|---|---|---|
| **Octocode MCP** (`octocode-mcp`, bundled) | The research engine: local search and reading, structure and AST search, LSP, GitHub repo/code/PR/history search, packages | Registered with Pi's MCP as server `octocode`; its read tools appear as `mcp__octocode__*`, and the prompt routes research to them. Override it with a server named `octocode` in Pi's `mcp.json`, or turn it off with `OCTOCODE_MCP=0`. |
| **Octocode skills** | Reusable workflows such as `octocode-research`, `octocode-architect`, `octocode-agents-communication`, `octocode-documentation`, and `octocode-chrome-devtools` | Loaded from `~/.octocode/skills` and, in trusted projects, `<repo>/.octocode/skills`, next to Pi's, Claude Code's and Codex's skill folders. Install one with `npx -y octocode skill install <name>`. |
| **Agent model** | The octocode-agents-communication model: agents, messages, deliveries and file leases | Backs `coordinate`, `sendMessage`, the `/agents` panel and subagent hand-offs. |
| **Subagent profiles** | Markdown profiles: bundled `implementer`, `researcher`, `reviewer`, `webHeadless`, `webLive` | Add your own in `~/.octocode/agents/` or `<repo>/.octocode/agents/` (Pi's `~/.pi/agent/agents/` and `.pi/agents/` work too). |
| **Octocode home** (`~/.octocode`, `OCTOCODE_HOME`) | User config: `skills/`, `agents/`, `hooks.json`. Runtime state in `agent/pi/`: the agent database `octocode.db` (sessions, backlog, memory, team), per-session output, bash logs and checkpoints | One SQLite file shared by every Pi session on the machine, so the backlog, memory and team are visible across sessions and subagents. |
| **Workspace** (`<repo>/.octocode`) | Project config: `skills/`, `agents/`, `hooks.json` (trusted projects only), plus `tmp/` for subagent hand-off docs and `backlog.md` exports | Resolved from the repository root, so a session started in a subfolder sees the same files. |

See [CONFIGURATION.md](docs/CONFIGURATION.md#where-files-go) for every path and variable.

## Quick start

1. Install the extension and start Pi in a repository:

   ```bash
   pi install npm:@octocodeai/pi-extension
   cd my-repo && pi
   ```

2. Type `/octocode` to check the setup. It shows the version, the number of Octocode MCP tools, the subagent profiles, the loaded skills and whether hooks are on.

3. Ask a research question, for example `Where is the session token refreshed, and who calls it?` The agent searches with the `mcp__octocode__*` tools, reads only the lines it needs, and cites `path:line`.

4. For GitHub research, export a token before you start Pi (for example `GITHUB_TOKEN`). The Octocode MCP server inherits Pi's environment.

## How Octocode extends Pi

Octocode uses Pi's extension API and keeps Pi's behavior where Pi already does the job.

| Pi feature | What Octocode adds | Pi API used |
|---|---|---|
| MCP | Registers the Octocode research server: local code search, file reading, structure and AST search, LSP callers, references and types, GitHub repo, code, PR and history search, packages. The GitHub and package tools are deferred behind Pi's `tool_search` to save prompt tokens (`OCTOCODE_MCP_DIRECT=1` exposes them all). `/mcp`, reconnects and OAuth stay Pi's. | `registerMcpServer` |
| System prompt | A short `octocode` section: investigate before changing, verify for real, prefer Octocode MCP for research, when to delegate. Pi's tool list, `AGENTS.md` context and skills are unchanged. | `before_agent_start` prompt sections |
| `edit` / `write` | One `file` tool batches edits, writes and deletes, each with a reasoning line, on Pi's own edit and write engines. It refuses stale edits and writes atomically. It replaces `edit` and `write` once per session, unless you name them in `--tools` or `defaultTools`. | `createEditToolDefinition`, `createWriteToolDefinition` |
| `bash` | Pi's own bash with your `shellPath` and `shellCommandPrefix`, plus a 15-minute deadline, background jobs and a guard against catastrophic commands. | `createBashToolDefinition`, `tool_call` |
| Compaction | Pi writes the summary with your model. Octocode trims old, large tool results and adds the files its `file` tool changed to Pi's file lists. | `session_before_compact` |
| Sessions | `/sessions` lists Pi's sessions with branch, cost and files changed, and resumes them with a short brief. Pi's `/resume` and session names are untouched. | `SessionManager.list`, `switchSession` |
| Skills | Also loads skills from `~/.claude/skills`, `~/.codex/skills` and `~/.octocode/skills`, plus project skills in trusted projects. Claude.ai `synced/` skills under `~/.claude/skills` are skipped: they need connectors Pi does not have. | `resources_discover` |
| Project trust | Gates Claude Code and Codex hooks, project agent profiles and project skills behind `/octocode trust`, because Pi's trust covers only Pi's own files. | `isProjectTrusted` |
| UI | Octocode replaces Pi's header with the gradient "OCTOCODE CODE" banner (one light sweep at startup) and its footer with a two-line one (model, thinking, context bar, review mode, running agents and bash jobs, `tok/s`, cost; cwd, branch, session name, tokens, other statuses), and adds a gradient spinner, a `✻ Thinking…` label for hidden thinking, an `octocode · <dir>` title, a working line that names the running step, a `✉ n queued` status, a desktop notification and title `●` when an answer is ready while the terminal is in the background, an agents panel while subagents run, one-at-a-time queueing for its dialogs, and compact renderers for its tools. | `setHeader`, `setFooter`, `setWorkingIndicator`, `setHiddenThinkingLabel`, `setStatus`, `setWorkingMessage`, `setWidget`, `setTitle`, `ui_prompt_start`, tool renderers |

## Features

Each feature has a full description in [docs/FEATURES.md](docs/FEATURES.md).

### Research and code

- **Octocode MCP**: research tools named `mcp__octocode__*`. Local: `localSearch`, `localFetch`, `structureSearch`, `astSearch` (`astTopology` with `OCTOCODE_BETA=1`). LSP: `lspSearch`. GitHub: `ghSearchRepo`, `ghSearchCode`, `ghStructure`, `ghGetFileContent`, `ghSearchHistory`, `ghGetHistoryItem`. Packages: `artifactSearch`. Classification: `clasify` (with `OCTOCODE_CLASSIFICATION_API`). `ghCloneRepo` and `astRewrite` are CLI-only.
- **`file` tool**: batched edit, write and delete with a reasoning line per change. A change to a file that moved on disk since the agent read it is refused until it reads the file again. Writes are atomic and keep the file mode.
- **Checkpoints**: before `file` changes a path, its bytes are saved. `/octocode rewind [turns]` restores files the agent changed, but never overwrites files you changed since.
- **File review** (opt-in): `/octocode review on` asks before every `file` batch, with Apply all, Review one by one, or Reject all.
- **Bash**: streaming output, a deadline, and `background: true` jobs whose exit status and log tail arrive later as a message. `/octocode jobs` lists and stops them.

### Delegation and teamwork

- **Subagents**: the `agent` tool runs a task in a fresh Pi process, up to 3 at once. Bundled profiles: `researcher`, `reviewer`, `implementer`, `webHeadless` and `webLive`.
- **Background subagents**: `background: true` returns at once. The report arrives as a message when the subagent finishes, so the agent never polls.
- **Isolation**: `isolate: true` runs a subagent in a private git worktree and commits its changes to `refs/octocode/pi/<id>`. `/agents merge <id>` merges them.
- **Team**: agents in one repository see each other with `coordinate`, reserve files before editing, and message each other with `sendMessage`. An idle agent wakes only for a message that needs a reply.

### Backlog, memory and sessions

- **Backlog**: a task board per repository (`backlog` → `todo` → `ongoing` → `done`). The agent tracks work that outlives a turn; `/backlog` opens the board and `/backlog do <id>` hands an item to the agent.
- **Memory**: durable notes (preferences, facts, decisions, gotchas, procedures) per repository or global, with full-text search. Relevant notes are injected before each prompt, labelled as data.
- **Session resume**: `/sessions` lists this repository's sessions. Resuming one adds a brief when the branch moved, backlog items are ongoing, or you were away over an hour.

### Working in the TUI

- **Working line**: names the current step: `Thinking`, `Writing`, `Running bash yarn test`, `Running 3 tools: bash · localSearch ×2`, `Checking bash …` while pre-run checks and PreToolUse hooks run, or `Waiting for you: <title>` during a dialog. After 2 s it adds the elapsed time. The agents panel shows the same line as the agent's activity.
- **Parallel tool calls**: tools batched in one message run in parallel. `file`, `browser`, `askUser`, `memory` and `backlog` keep their own calls in order with a per-tool lock, so MCP searches and bash in the same batch never wait behind them.
- **One dialog at a time**: Pi shows one extension dialog at a time, and a second one would leave the first unanswered forever ([pi#6978](https://github.com/earendil-works/pi/issues/6978)). Octocode queues every dialog it opens (`askUser`, file review, browser, memory and trust confirms, `/octocode` pickers). The working line shows `(+N queued)` while others wait, and a cancelled call leaves the queue.
- **Wait times in tool rows**: a row whose call waited more than 250 ms before it ran shows `checks 1.2s` (its own reservation, bash-guard and PreToolUse checks) or `queued 2.0s` (other calls' checks or the tool's lock), next to its run time.
- **Queued messages survive Esc**: messages that arrive while the agent works (subagent reports, background bash reports, teammate messages) show as `✉ n queued`. Pi's Esc clears its queue but restores only your typed text, so Octocode adds the cleared messages back to the conversation without starting a turn and says so. This applies in the TUI only.
- **Answer-ready notifications**: when an answer finishes or a dialog opens while the terminal is in the background, Octocode sends a desktop notification and puts `●` in the terminal title until you return.
  - It sends OSC 9 for iTerm2, OSC 777 for Ghostty and WezTerm, OSC 99 for Kitty and the bell elsewhere, wrapped for tmux.
  - It knows the terminal is in the background from the terminal's focus reports. In fullscreen mode it uses idle time instead: a run of at least 30 s with no key pressed for 30 s.
  - Subagents, aborted runs and print, JSON and RPC modes never notify.
  - Set `OCTOCODE_NOTIFY=always` to notify while focused, or `off` to never notify.

### Web

- **`web`**: fetches a page as readable text, or searches the web (Tavily, Serper, Exa or Brave with an API key; DuckDuckGo and Bing without). Private and cloud-metadata addresses are refused.
- **`browser`**: drives Chrome over DevTools for JavaScript pages, forms, uploads and screenshots. Private addresses and uploads outside the workspace need your confirmation. It never attaches to your Chrome unless you opt in.

### Integrations

- **Hooks** (opt-in): with `OCTOCODE_HOOKS=1`, Claude Code and Codex command hooks run on Pi events (`PreToolUse`, `PostToolUse`, `SessionStart`, `PreCompact`, `Stop`, `Notification`). Existing Claude hooks work unchanged.
  - **Blocking:** `PreToolUse` can block a call. A `Stop` hook that returns `{"decision":"block","reason":…}` continues the finished run with that reason. As in Claude Code, it gets `stop_hook_active` and is capped at 8 continuations in a row.
  - **Notification:** `Notification` hooks fire on `permission_prompt`, `elicitation_dialog` and `idle_prompt`. They run in the background and never block.
  - **Running:** matching hooks run in parallel, and a hook listed in several files runs once.
  - **Timing:** Pi checks every call of a batch before any of them runs, so a slow `PreToolUse` hook holds up the whole batch. A hook over 5 s is reported once, and `/hooks` shows each command's run count, average and slowest time.
- **External API** (opt-in): with `OCTOCODE_API=1`, other programs message the session and follow its events over JSON-RPC. `tool.start` and `tool.end` events carry `depth` (0 for the agent's own calls) and, for calls made by another tool, `parentId`. `tool.end` also carries `durationMs`. `octocode-pi-api watch --top-level` drops the nested calls. See [docs/API.md](docs/API.md).
- **`askUser`**: the agent asks 1 to 4 multiple-choice questions in an inline dialog when a choice depends on you.

## Tools the agent gets

| Tool | Purpose |
|---|---|
| `read` | Pi's read tool, unchanged |
| `bash` | Shell commands, with a deadline and background jobs |
| `file` | Batched edit, write and delete |
| `mcp__octocode__*` | Code, LSP, GitHub and package research |
| `web` | Fetch a URL or search the web |
| `browser` | Drive Chrome over DevTools |
| `agent` | Run a subagent |
| `coordinate` | List agents, reserve files, see recent changes |
| `sendMessage` | Message another agent |
| `backlog` | Track repository tasks |
| `memory` | Search and save durable notes |
| `askUser` | Ask you multiple-choice questions |

Subagents get a reduced set: no `agent` or `askUser`, and each profile drops what it doesn't need. For example, `researcher` and `reviewer` have no `file`.

## Commands

Everything lives under `/octocode`; tab completes subcommands. `/sessions`, `/backlog`, `/memory` and `/hooks` are shortcuts. `/mcp` is Pi's own MCP command.

| Command | Does |
|---|---|
| `/octocode` | Status: version, Octocode MCP tools, profiles, skills, hooks |
| `/octocode rewind [turns]` | Undo the `file` changes of the last turns (default 1) |
| `/octocode review on\|off` | Ask before every `file` batch |
| `/octocode jobs [kill <id>]` | List or stop background bash jobs |
| `/octocode trust [off]` | Review and trust this project's hooks, agent profiles and skills |
| `/octocode api [on [http] \| off]` | Start or stop the external API |
| `/agents [tell <id\|all> <text> \| kill <id> \| merge <id>]` | List the team, message agents, stop or merge a subagent (shortcut for `/octocode agents`) |
| `/sessions [all \| <words>]` | Find and resume sessions |
| `/backlog [add <title> \| <id> [state] \| do <id> \| export]` | Open, edit or work the board |
| `/memory [search <words> \| add [title] \| auto on\|off \| last]` | Browse, search and add memories |
| `/hooks` | Loaded hooks, the files to edit, and each hook's runs, average and slowest time |

Without a UI (print or JSON mode), these commands write text to stderr instead of opening a picker.

## Best practices

**Ask for evidence, not guesses.** Phrase research as a question about the code ("who calls X?", "why does Y fail?"). The agent searches with Octocode MCP and reads only the lines it needs. Before a change to shared code, ask it to confirm callers with `lspSearch`.

**Delegate wide work, keep narrow work.** Ask for parallel subagents when the work splits cleanly: one per module to review, or research next to implementation. Small sequential edits are faster in the main session. Ask for `isolate: true` when a subagent's edits might collide with yours.

**Let background work come back to you.** Long builds belong in `bash` background jobs and long tasks in background subagents. Results arrive as messages; nothing needs polling.

**Keep the backlog for follow-ups.** Say "add a backlog item to…" for work you discover and leave for later. Use `/backlog do <id>` to start one later in any session.

**Teach it once with memory.** Say "remember that we use pnpm" or "never edit generated files" and it saves a memory that is injected in later sessions. Store where a secret lives, never the secret itself; credential-like text is refused.

**Review risky edits.** Turn on `/octocode review on` in unfamiliar repositories, and use `/octocode rewind` to undo a turn's `file` changes.

**Trust deliberately.** Run `/octocode trust` only after reading what it lists: project hooks run commands with your permissions.

**Split dependent steps.** Calls in one message run in parallel, so put an edit and the test that checks it in separate messages. Keep `PreToolUse` hooks fast: each one delays every call in its batch. A row's `checks Ns` shows how long.

**Keep the context lean.** Prefer `web` for static pages and `browser` only for JavaScript or interaction. Use the `webHeadless` profile for multi-page browsing so screenshots and snapshots stay out of the main context.

## Configuration

Most users need nothing. Common settings:

| Variable | Effect |
|---|---|
| `GITHUB_TOKEN` | GitHub access for Octocode MCP |
| `OCTOCODE_HOOKS=1` | Run Claude Code and Codex hooks |
| `OCTOCODE_REVIEW=1` | Start with file review on |
| `OCTOCODE_NOTIFY=off\|always` | Answer-ready notifications (default: only when the terminal is in the background) |
| `OCTOCODE_NOTIFY_METHOD=osc9\|osc777\|osc99\|bel` | Force a notification method (default: picked from the terminal) |
| `OCTOCODE_API=1` | Serve the external JSON-RPC API on a Unix socket |
| `OCTOCODE_MAX_SUBAGENTS` | Subagents allowed at once (default 3) |
| `OCTOCODE_MCP=0` | Don't register the Octocode MCP server |
| `TAVILY_API_KEY`, `SERPER_API_KEY`, `EXA_API_KEY`, `BRAVE_API_KEY` | Web search providers |
| `OCTOCODE_HOME` | Where Octocode keeps config and state (default `~/.octocode`) |

Other MCP servers are configured with Pi: `pi mcp add`, `~/.pi/agent/mcp.json` or `.pi/mcp.json`. A server named `octocode` there overrides the built-in one. With `pi --no-extensions`, add `-e builtin:mcp -e builtin:tool-search` to keep Pi's MCP and tool search, or the Octocode tools (or its deferred GitHub and package tools) don't load.

Every variable, file location, trust rule, hook and profile format is in [docs/CONFIGURATION.md](docs/CONFIGURATION.md).

## Documentation

| Page | Covers |
|---|---|
| [docs/FEATURES.md](docs/FEATURES.md) | Detailed behavior of every feature |
| [docs/CONFIGURATION.md](docs/CONFIGURATION.md) | Environment variables, file locations, trust, hooks, subagent profiles, MCP, retention |
| [docs/API.md](docs/API.md) | External JSON-RPC API and the `octocode-pi-api` command |
| [docs/COMPACTION.md](docs/COMPACTION.md) | How compaction trims context |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | Source layout, for contributors |

## Development

```bash
yarn workspace @octocodeai/pi-extension build   # tsc → dist/
yarn workspace @octocodeai/pi-extension test    # unit and end-to-end tests
pi --no-extensions -e packages/octocode-pi-extension/dist/index.js -e builtin:mcp -e builtin:tool-search
```

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the source layout.
