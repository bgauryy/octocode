# `@octocodeai/pi-extension`

The official Octocode package for Pi. It combines Octocode research through MCP with guarded file and shell operations, subagents, skills, media workflows, planning, and a live settings interface.

The Pi host SDK (`@earendil-works/pi-coding-agent` 0.84.4) is a required peer dependency because the extension imports its runtime APIs.

## Install

Requires Node.js 22.22.2+ (22.x), 24.15.0+ (24.x), or 26+.

```bash
pi install npm:@octocodeai/pi-extension
```

The package registers its extension and themes through Pi's package manifest. Octocode's built-in MCP server is configured automatically, preferring the package-local server and falling back to the version range owned by [`mcp/config.ts`](src/tools/mcp/config.ts).

## Runtime surface

Configuration uses `~/.octocode` globally and `.agents` in the workspace. Pi defaults
are used **only for models** (`~/.pi/agent/models.json`, honoring `PI_CODING_AGENT_DIR`).
See [capability sources and precedence](docs/CAPABILITIES.md#source-paths).

The live source inventory is authoritative. Use `/config` inside Pi to open the configuration page in your OS browser, and see [HARNESS.md](HARNESS.md) for the detailed contract. `/configuration` is an alias.

| Surface | Count |
|---|---:|
| Native Octocode research tools | 0 (research is served through `MCPTool`) |
| Pi support tools | 14 |
| Guarded Pi builtin overrides | 1 (`bash`) |
| Disabled Pi builtins | 6 |
| Slash command entries | 5 |
| Bundled main-agent skills | 14 |

### Support tools

| Tool | Purpose |
|---|---|
| `file` | Create, edit, or delete files through one guarded mutation boundary. |
| `web` | Fetch an absolute URL or search the web. |
| `chromeDebug` | Inspect and control Chrome through CDP. |
| `agent` | Spawn and manage researcher, architect, planner, implementer, read-only reviewer, browser, and explicit custom workers. |
| `callTool` | Invoke a capability from the live dynamic-tool registry. |
| `skill` | Load and manage installed skills. |
| `plan` | Manage session and shared plans with verification receipts. |
| `localServer` | Serve an inspected local directory on loopback for review. |
| `awareness` | Discover, describe, and invoke the canonical Awareness command runtime without shell syntax. |
| `MCPTool` | Discover, describe, call, and manage MCP tools and servers. |
| `askUser` | Request structured input through Pi's UI. |
| `inspectMedia` | Inspect images, video, and audio. |
| `media` | Create or transform media and PDFs. |
| `runFfmpeg` | Run guarded ffmpeg or ffprobe argument lists. |

The extension overrides `bash` with command and path guards. It removes Pi's public `read`, `edit`, `write`, `grep`, `find`, and `ls` tools; use Octocode research tools for reads and discovery, and `file` for mutations.

Awareness coordination uses the native `awareness` facade for catalog discovery and
host-bound command calls through the imported Awareness API. The bundled skill provides workflow guidance. Pi
supplies the database, workspace and agent identity; native calls never launch the Awareness CLI. Signals,
locks, memory, bookkeeping and maintenance share the same SQLite ledger as native
Pi events and external CLI agents. Pi retains automatic registry/event delivery,
mutation guards and plan UI. Peers use the same physical database with distinct stable
IDs; workers keep their own physical worktree as the workspace for file and lock
ownership. `OCTOCODE_AWARENESS_DB` is the canonical inherited binding across native
calls, the guarded CLI fallback, worker lifecycle and delivery; no database copies are
created. See [Awareness agent flow](docs/AWARENESS_AGENT_FLOW.md).

The default Awareness flow is one peer briefing plus native message delivery. Scheduled status checks require `OCTOCODE_CRON_STATUS=1`. Work bookkeeping and worker audits require the guard/full workspace profile; full enables bounded local file history around native `file` mutations with bundled private Git storage. `/octocode-rewind` previews and explicitly applies a selected file restore in interactive Pi; headless sessions use the same `history` commands through the native `awareness` facade. This does not snapshot the workspace on every prompt or rewind the conversation.

Workers receive versioned grants drawn from the parent's enabled native tools, exact skill source IDs, and MCP server/tool pairs. The parent can replace a worker's selections with `agent type:"configure"`; removals take effect immediately and additions become available before its next turn. Custom workers require an explicit tool allowlist and a non-empty role prompt. See [capability and grant contracts](docs/CAPABILITIES.md#worker-grants).

The prompt and effective catalog refresh at the next turn boundary. MCP discovery uses bounded `list` pages followed by `describe` for exact schemas; skills use bounded `list` pages followed by `load`. Continuations carry the catalog revision so changes cannot silently mix two inventories.

## Configuration and privacy

Octocode configuration is shared by the CLI, MCP server, and this extension. Resolution order is:

```text
environment variables > <octocode-home>/.octocoderc > built-in defaults
```

Existing `.octocoderc` files remain valid. The optional storage setting is backward-compatible; omitting it keeps persistent behavior.

```json
{
  "version": 1,
  "storage": {
    "mode": "memory"
  }
}
```

Set `OCTOCODE_HOME` to change the Octocode home directory. Set `OCTOCODE_STORAGE_MODE=memory` for an environment override. In memory mode, Octocode disables response-disk caching, clone and exact-file materialization, session/stat persistence, and the extension's SQLite-backed Awareness state. The extension keeps active interaction and authorization state in process memory until exit. It does not delete existing files or disable user-authored configuration and credentials. Invalid environment values do not weaken a valid `.octocoderc` memory setting.

Native workspace capabilities live in `.agents/`: `mcp.json`, `models.json`, `skills/`, `hooks/`, and optional `AGENTS.md`. Global Octocode sources use the corresponding paths under `OCTOCODE_HOME`. Pi resources remain available, including `PI_CODING_AGENT_DIR` and explicit skill paths. Claude, Codex, and Cursor MCP/skill definitions appear as disabled candidates until their exact source revision is reviewed and linked. [Capability sources](docs/CAPABILITIES.md) documents precedence, trust, imports, models, and command hooks.

See the repository [configuration guide](https://github.com/bgauryy/octocode/blob/main/docs/CONFIGURATION.md) for every supported key and [docs/SETTINGS.md](docs/SETTINGS.md) for Pi's control center, persistence, and security behavior.

## Slash command entries (5)

| Command | Purpose |
|---|---|
| `/config` | Open the configuration page in the OS browser. |
| `/octocode-rewind` | Preview and explicitly apply a local file-history restore. |
| `/octocode-inbox` | Inspect, steer, or stop spawned workers through a keyboard-driven picker. |
| `/octocode-status` | Inspect session usage, tools, skills, plan, agents, and pending decisions; `events` shows the selected branch’s execution journal and `export` saves JSONL. |
| `/configuration` | Alias for `/config`. |

The footer separates live activity from session metadata. It lists active workers and briefly retains done/killed outcomes before their full detail remains in `/octocode-inbox`. Plan updates, worker state transitions, and parent↔worker messages appear as durable transcript cards without entering model context. `/octocode-status` opens details; `/octocode-status export` writes the semantic journal into the session artifact directory. Full messages and tool output remain in Pi’s transcript. The footer shows `/config`. The page controls MCP connections and tools,
skill sources, command hooks, permissions, theme, effort, and footer density. It also shows models and worker grants and opens the current plan
for review. Host-provided and user-installed commands remain in the live inventory.
The recovery command remains preview-first and does not rewrite input through regex triggers.

## Bundled skills (14)

The build copies these main-agent skills into `dist/skills/`:

- `octocode-architect`
- `octocode-awareness`
- `octocode-brainstorming`
- `octocode-chrome-devtools`
- `octocode-clean-agentic-code`
- `octocode-documentation`
- `octocode-eval-benchmark`
- `octocode-prompt-optimizer`
- `octocode-research`
- `octocode-rfc-generator`
- `octocode-roast`
- `octocode-scraping`
- `octocode-skills`
- `octocode-subagent`

## Documentation

- [HARNESS.md](HARNESS.md): full registered surface and lifecycle
- [docs/TOOLS.md](docs/TOOLS.md): tool routing and MCP usage
- [docs/SETTINGS.md](docs/SETTINGS.md): settings, persistence, and security
- [docs/CAPABILITIES.md](docs/CAPABILITIES.md): source paths, linked imports, models, hooks, and worker grants
- [docs/SUBAGENTS.md](docs/SUBAGENTS.md): worker profiles and coordination
- [docs/MEDIA_TOOL.md](docs/MEDIA_TOOL.md): media routing
- [docs/FFMPEG.md](docs/FFMPEG.md): ffmpeg and ffprobe workflows
- [docs/README.md](docs/README.md): complete package documentation index

## Development

From the monorepo root:

```bash
yarn workspace @octocodeai/pi-extension build
yarn workspace @octocodeai/pi-extension test
yarn workspace @octocodeai/pi-extension typecheck
```

The package's production test suite checks the documented inventories against the executable harness so counts and names cannot drift silently. Conformance helpers (`createProductionPiScenarioSuite`, `captureProductionPiLifecycle`) are test-only: they live in `tests/helpers/production-pi.ts` and are not published, so the package exposes no conformance subpath.

Awareness uses one compact cooperation policy with full CLI/skill detail on demand. See [agent flow](docs/AWARENESS_AGENT_FLOW.md#final-worker-audits-and-context-estimates) for bounded wake-ups, final worker debt audits and context-estimate limits.
