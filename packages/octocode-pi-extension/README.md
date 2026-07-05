# @octocodeai/pi-extension

<div align="center">
  <img src="https://github.com/bgauryy/octocode-mcp/raw/main/packages/octocode-pi-extension/assets/logo.png" width="640px" alt="Octocode + Pi">
</div>

Octocode for [Pi](https://github.com/earendil-works/pi): one package that gives the agent native code-research tools, live web search, persistent memory, edit-safety hooks, and bundled skills.

The Octocode CLI is **not bundled**. For CLI workflows (auth, search, clone, cache, install, lsp-server) install separately:

```bash
npm install -g octocode   # or: npx octocode <command>
```

```bash
pi install npm:@octocodeai/pi-extension
/octocode-status
```

---

## What happens after install

When Pi loads the extension, it does four things:

1. **Adds the Octocode operating model to the system prompt.** The prompt tells the agent to research before changing code, verify claims, use memory, protect secrets, manage context, and run the project’s real checks. The prompt block is idempotent — it is appended once, not duplicated.
2. **Registers native tools.** Code/GitHub/npm/local/LSP/OQL tools execute directly through `@octocodeai/octocode-tools-core`; no MCP server is spawned for these calls.
3. **Registers support tools.** Web search, context management, handoff, and memory tools are available as Pi tools.
4. **Registers slash commands.** A lean set of harness commands (`/octocode-status`, `/octocode-harness`, `/octocode-setup`, `/octocode-skills-update`). For CLI workflows use `npx octocode` directly.

During normal use:

- Before Pi write/edit tool calls, the awareness bridge claims locks for target files.
- After the edit tool result, the bridge releases locks and leaves an intent in `PENDING`, meaning verification is still owed.
- The system prompt tells the agent to run the stated check and clear pending intents. When the awareness stop hook is installed/active, it can block conclusion while `PENDING` intents remain.
- Memory tool calls go directly into `@octocodeai/octocode-memory` and use a local SQLite DB under Octocode memory home.
- Skills are bundled from this package and loaded by Pi like normal Agent Skills.

What does **not** happen automatically:

- No repository is cloned unless the agent/user invokes clone/cache tooling.
- No destructive command runs without the normal Pi/tool confirmation path.
- No GitHub token is read from `.env`; tokens use shell env or Octocode auth storage.
- The Octocode CLI is not bundled in this package. Use `npx octocode` or install it globally: `npm install -g octocode`.

---

## Quick start

```bash
pi install npm:@octocodeai/pi-extension
/octocode-status        # health: prompt, skills, memory, native tools, web provider
/octocode-harness       # exact list of registered tools, commands, and skills
```

For a project-specific persistent prompt block:

```bash
/octocode-setup         # writes .pi/APPEND_SYSTEM.md in the current project
/octocode-setup --global # writes ~/.pi/agent/APPEND_SYSTEM.md
```

Use `/octocode-setup` when you want Octocode’s operating model pinned even if the package is not actively loaded.

---

## Everyday workflows

### Research code

The agent should use native tools first:

- locate files/symbols with local or GitHub search,
- inspect structure before reading raw files,
- use LSP semantics when symbols/types matter,
- use exact file reads only after it has anchors.

For command-style workflows, use `npx octocode` in a terminal:

```bash
npx octocode search "where is auth handled?"
npx octocode context tools
npx octocode cache repo owner/name
npx octocode clone owner/name
npx octocode lsp-server status
```

### Use live web

The `web` tool searches the web or fetches clean page text. It is for current external information: docs, changelogs, errors, model/provider limits, and news.

Provider order:

1. Tavily, if `TAVILY_API_KEY` or `TAVILY_API_TOKEN` is set
2. Serper, if `SERPER_API_KEY` is set
3. DuckDuckGo fallback, no key required

### Remember durable lessons

The agent has concise typed memory tools: `memory_recall`, `memory_record`, `memory_reflect`, `memory_workspace_status`, `memory_refine_get`, `memory_audit_unverified`, `memory_verify`, and `memory_digest`.

Each tool has a narrow schema so the agent can pick the right memory action without a `type` discriminator. Memory is intentionally selective: good memories are reusable lessons, root causes, decisions, recurring failure signatures, and verified workarounds. Attach `file`, `files`, `folders`, or repo-wide `repo` / `workspace_path` scope so recall can connect lessons to code, docs, README, AGENTS.md, and architecture files. Bad memories are routine status, raw logs, secrets, test output, or facts already captured by git/docs. `memory_record` checks similar active memories and skips duplicate captures unless the agent passes `supersedes` or explicitly sets `allow_similar:true` for distinct new evidence; `valid_to` plus `memory_digest` marks expired memories stale.

### Coordinate long work

The agent can manage context and parallel workers without user babysitting:

- `compact_context` summarizes long history when the next phase needs room.
- `clear_context` starts a fresh unrelated session when Pi exposes session control to the command context; otherwise use `/new` manually.
- `spawnAgent` starts a background Pi worker process over RPC and returns an `agentId`. Workers cannot spawn other workers; orchestration stays in the parent session.
- `AgentMessage` lists, checks, messages, waits for, or kills spawned workers.

---

## Configuration

Octocode config is optional. The extension loads Octocode env/config from the shared Octocode config system.

Common locations:

```bash
~/.octocode/.env           # global
<project>/.octocode/.env   # project-local
```

Useful keys:

```bash
TAVILY_API_KEY=tvly-...    # better web search
SERPER_API_KEY=...         # Google SERP web search
OCTOCODE_WEB_USER_AGENT=...# optional web fetch user-agent override
```

GitHub auth:

- Use shell env (`GITHUB_TOKEN`, `GH_TOKEN`, `OCTOCODE_TOKEN`) or Octocode auth commands.
- Do not put GitHub tokens in `.octocode/.env`; protected keys are not imported from project env files.

CLI auth commands (run in a terminal):

```bash
npx octocode auth status
npx octocode login
npx octocode logout
npx octocode status
```

Full config docs: [CONFIGURATION.md](https://github.com/bgauryy/octocode-mcp/blob/main/docs/CONFIGURATION.md) · [AUTHENTICATION.md](https://github.com/bgauryy/octocode/blob/main/docs/AUTHENTICATION.md)

---

## Model configuration (`models.json`)

Custom Pi models live in `~/.pi/agent/models.json`. The file hot-reloads when you open `/model`.

The important rule:

> Set `contextWindow` and `maxTokens` to the provider’s real published limits.

`contextWindow` controls context warnings and compaction. `maxTokens` is the hard output cap Pi sends to the provider. If `maxTokens` is too low, the model stops mid-answer even when the context window is large.

Minimal OpenAI-compatible provider:

```json
{
  "providers": {
    "my-provider": {
      "baseUrl": "https://api.example.com/v1",
      "api": "openai-completions",
      "apiKey": "$MY_API_KEY",
      "compat": {
        "supportsDeveloperRole": false,
        "supportsReasoningEffort": false
      },
      "models": [
        {
          "id": "model-id",
          "name": "Human label",
          "contextWindow": 131072,
          "maxTokens": 65536,
          "reasoning": false,
          "input": ["text"]
        }
      ]
    }
  }
}
```

Fields that matter:

| Field | Effect |
|---|---|
| `contextWindow` | Pi context accounting and compaction timing |
| `maxTokens` | Maximum generated tokens per response |
| `reasoning` | Enables Pi thinking controls for models that support them |
| `input` | Include `"image"` only for vision-capable models |
| `compat.supportsDeveloperRole` | Set `false` for servers that reject the OpenAI `developer` role |
| `compat.supportsReasoningEffort` | Set `false` for servers that reject `reasoning_effort` |
| `compat.supportsUsageInStreaming` | Set `false` for servers that reject streaming usage options |

API key forms:

```json
"apiKey": "$MY_API_KEY"
"apiKey": "!op read 'op://vault/item/credential'"
"apiKey": "literal-key-avoid-in-shared-files"
```

After editing:

```text
/model            # model appears
/octocode-status  # extension still loads cleanly
```

Full Pi model docs: [models.md](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/models.md)

---

## Registered surfaces

This section is intentionally explicit. `/octocode-harness` shows the same information from the running extension.

### Native Octocode tools

| Tool | Use |
|---|---|
| `ghSearchCode` | Search GitHub code |
| `ghSearchRepos` | Search GitHub repositories |
| `ghHistoryResearch` | Research GitHub history, PRs, commits, and changes |
| `ghGetFileContent` | Read GitHub file contents |
| `ghViewRepoStructure` | Inspect GitHub repository structure |
| `ghCloneRepo` | Clone/materialize GitHub repos when enabled |
| `localSearchCode` | Search local code |
| `localFindFiles` | Find local files |
| `localGetFileContent` | Read local files |
| `localViewStructure` | Inspect local directory/repo structure |
| `lspGetSemantics` | Query language-server semantics |
| `localBinaryInspect` | Inspect binary/artifact metadata |
| `npmSearch` | Search npm/package metadata |
| `unzip` | Unpack archives for local research (routes to localBinaryInspect mode:"unpack") |

### Support tools

| Tool | Use |
|---|---|
| `web` | Search live web or fetch clean page text |
| `compact_context` | Summarize current conversation to free context |
| `clear_context` | Start a fresh unrelated session when session control is available |
| `spawnAgent` | Start a background Pi worker process over RPC; recursive worker spawning is disabled |
| `AgentMessage` | List, check, message, wait for, or kill spawned workers |
| `memory_recall` | Recall durable lessons before risky, unfamiliar, or long-running work |
| `memory_record` | Store a durable root cause, decision, workaround, or verified gotcha |
| `memory_reflect` | Capture a reusable lesson after non-trivial work |
| `memory_workspace_status` | Show active file locks, working agents, and memory store stats for the current workspace |
| `memory_refine_get` | List open repo-fix refinements for the current workspace |
| `memory_audit_unverified` | List pending edit intents that still need verification |
| `memory_verify` | Mark a pending edit intent as verified or failed |
| `memory_digest` | Review, deduplicate, and prune the memory store; supports `dry_run` preview and `export_doc` markdown report |

### Slash commands

The extension registers a small set of harness commands. For CLI workflows (auth, search, clone, cache, install, skill, lsp-server, context) use `npx octocode` directly in a terminal.

| Command | Use |
|---|---|
| `/octocode-status` | Show extension health: prompt, skills, memory, tools, web |
| `/octocode-harness` | List every registered surface |
| `/octocode-setup` | Install/update the managed APPEND_SYSTEM block |
| `/octocode-skills-update` | Update this Pi package and reload resources |

### Bundled skills

| Skill | Use |
|---|---|
| `octocode-research` | Evidence-first investigation, planning, implementation, review, and refactor work |
| `octocode-brainstorming` | Validate ideas against evidence before building |
| `octocode-rfc-generator` | Produce RFCs/design docs for risky or cross-package changes |
| `octocode-roast` | Brutally honest code review with ranked findings |
| `octocode-prompt-optimizer` | Improve prompts, SKILL.md files, and agent instructions |
| `octocode-skills` | Find, evaluate, install, or author skills |
| `octocode-stats` | Inspect Octocode usage stats and savings |

---

## Troubleshooting

| Symptom | Check |
|---|---|
| Extension seems inactive | Run `/octocode-status`, then `/octocode-harness`. |
| Model stops mid-answer | Fix `maxTokens` in `~/.pi/agent/models.json`. |
| Model missing from `/model` | Check `apiKey`, provider `baseUrl`, and `api` value. |
| Web search quality is weak | Add `TAVILY_API_KEY` or `SERPER_API_KEY` to Octocode env. |
| GitHub calls are unauthenticated | Run `npx octocode login` or export `GITHUB_TOKEN`/`GH_TOKEN`/`OCTOCODE_TOKEN`. |
| Agent keeps using shell for code search | Remind it to prefer native Octocode tools; use `/octocode-harness`. |
| Verify gate blocks conclusion | Run the stated test plan, then mark the pending intent verified. If no stop hook is installed, pending intents still appear in memory/audit state but may not block the UI. |

---

## Development notes

Canonical sources:

- System prompt: `packages/octocode-pi-extension/docs/PI/APPEND_SYSTEM.md`
- Root skills: `skills/`
- Package/dist skills are regenerated by `packages/octocode-pi-extension/scripts/build.mjs`

Do not edit generated package skill copies as the source of truth; build will overwrite them.

Useful checks:

```bash
yarn workspace @octocodeai/pi-extension build
yarn workspace @octocodeai/pi-extension test
```

The test suite verifies that this README lists every live tool, command, and bundled skill from the extension harness.

---

[Octocode](https://octocode.ai) · [GitHub](https://github.com/bgauryy/octocode-mcp) · [Configuration](https://github.com/bgauryy/octocode-mcp/blob/main/docs/CONFIGURATION.md) · [Authentication](https://github.com/bgauryy/octocode/blob/main/docs/AUTHENTICATION.md) · [Pi packages](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/packages.md)
