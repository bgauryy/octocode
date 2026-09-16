# Capability sources and grants

The Pi extension uses the same model and declarative hook adapters in main and worker processes. The [capability adapter owner](../src/adapters/pi-capability-adapters.ts) initializes them after environment propagation, refreshes them before turns, and disposes their contributions when the session closes.

## Source paths

`OCTOCODE_HOME` selects the global Octocode directory through `@octocodeai/config`; its default is `~/.octocode`. `PI_CODING_AGENT_DIR` selects Pi's agent directory; its default is `~/.pi/agent`. `CODEX_HOME` selects the Codex directory for foreign discovery; its default is `~/.codex`. The table uses default global paths; the environment overrides relocate their corresponding sources.

Octocode uses `.pi` defaults **only for models**. `PI_CODING_AGENT_DIR` relocates that model source. Pi skill, MCP, and hook directories are not Octocode discovery sources; skills reported by the host from those default directories are excluded too. Explicit skill files outside those directories and bundled Octocode skills remain supported. Pi itself continues to own its host context and native extension lifecycle.

| Capability | Global Octocode source | Workspace source | Pi model integration / host ownership |
|---|---|---|---|
| Models | `~/.octocode/models.json` | `.agents/models.json` | `~/.pi/agent/models.json`, built-in providers, active authentication, and extension registrations |
| Declarative command hooks | `~/.octocode/hooks/*.json` | `.agents/hooks/*.json` | No Pi directory discovery; native extension handlers remain host-owned |
| Skills | `~/.octocode/skills`, `~/.agents/skills` | `.agents/skills` | No Pi directory discovery |
| MCP servers | `~/.octocode/mcp.json` | `.agents/mcp.json` | No Pi directory discovery |
| Instructions | `~/.agents/AGENTS.md`, `~/.octocode/AGENTS.md` | Ancestor `.agents/AGENTS.md` files within the repository | Pi's context files remain host-owned and are deduplicated by actual path |

The [shared path contract](../../octocode-agent-contracts/src/capability-sources.ts) owns global and workspace roots. [Skill discovery](../src/tools/skill-discovery.ts) and [MCP configuration](../src/tools/mcp/config.ts) own the complete imported-source inventories for their capabilities.

Discovery reads configuration data. It does not write Pi configuration, connect foreign MCP servers, execute hooks, or load another copy of Pi's native extensions. Workspace capabilities require current workspace trust.

MCP writes from `/config` and `MCPTool action:"add"` use `.agents/mcp.json` for project scope and `$OCTOCODE_HOME/mcp.json` for global scope. Retained native files have lower precedence: global `agent/mcp/servers.json` and `extension/mcp/servers.json`, workspace `extension/workspaces/<workspace-key>/mcp/servers.json`. Global definitions apply before workspace definitions; repository ancestors apply before the nearest directory, with `.agents/mcp.json` last at each workspace level. Existing Octocode skill locations also remain active below the native workspace sources. Skill authoring defaults to `$OCTOCODE_HOME/skills`.

Instruction files load from global sources first, then repository ancestors toward the current directory. The extension attributes their exact text as user-authority project instructions and excludes files Pi already loaded. Each file is bounded to 512 KiB; failures produce diagnostics. `--no-context` suppresses workspace instruction projection, and workers receive only their explicit worker context.

## Reviewed linked imports

Claude, Codex, and Cursor sources are candidates, disabled until reviewed. MCP locations include user `~/.claude.json`, `~/.claude/mcp.json`, `~/.cursor/mcp.json`, and `$CODEX_HOME/config.toml`; workspace locations include `.mcp.json`, `.claude/mcp.json`, `.cursor/mcp.json`, and `.codex/config.toml`. Claude's `~/.claude.json` project entries retain project scope. Foreign skill roots include the corresponding `.claude/skills`, `.cursor/skills`, and `.codex/skills` locations. Other recognized legacy hosts remain visible in the [discovery inventory](../../octocode-agent-contracts/src/mcp-discovery.ts), including global `~/.agents/mcp.json`; they follow the same review boundary.

`/config` opens the OS browser configuration page; `/configuration` is an alias. Review and link import records a stable source ID, the exact definition revision, and project or global scope. The original file remains authoritative. The extension does not copy it or rewrite the foreign application's configuration. A plain Enable action cannot authorize an unreviewed or changed definition.

Definition revisions are normalized hashes. MCP JSON/TOML parsing preserves supported transport, arguments, environment and header references, paths, authentication, timeouts, and tool filters. Unsupported fields or interpolation produce diagnostics and prevent activation. TOML uses the shared `smol-toml` parser. MCP source files must be regular, non-symlink files no larger than 1 MiB and unchanged during the read.

Changing a linked definition makes it pending review again. A removed reviewed source remains unavailable in the inventory. Restoring a source still requires an available, valid, trusted definition with the reviewed revision. Source identity is independent of display names and definition content; changing a skill directory's link target changes its revision. Review state lives in the extension-owned database under its scope and source identity, with in-memory state in memory storage mode.

## Effective skills

The effective inventory contains one valid, enabled skill per normalized name. Bundled skills win by default. Review and select source chooses an exact alternative by source ID and revision, including a replacement for a bundled name. Without that selection, ordinary workspace skills outrank global skills, and the nearest repository ancestor wins.

The configuration page retains all candidates: active, disabled, shadowed, pending review, unavailable, invalid, and untrusted. Name enablement is separate from source review and cannot bypass it. If a selected source changes or disappears, the selection stays pending or unavailable and that skill has no effective entry. Review the changed definition or explicitly select another source, including the bundle, to restore access.

Discovery visits nested skill directories and linked directories, tracks real paths to stop cycles, and verifies the source did not change while reading it. `SKILL.md` must pass the shared frontmatter schema and file bounds. Pi metadata without a valid file never creates a loadable skill. Explicit runtime files outside Pi default directories admit the selected file; package metadata follows the same boundary.

## Versioned prompt and catalogs

Before every turn, the extension resolves the current effective capabilities and replaces its owned prompt projection. An unchanged projection remains byte-stable; a changed catalog appears on the next turn without starting a new session. The host prompt, project instructions, and separate plan/recovery context retain their ownership.

The shared snapshot has `schemaVersion: 1` and a `revision` over enabled native tools, skill identities and revisions, and MCP server/tool contracts. Timestamps and credentials do not affect the revision. The prompt exposes `capability_revision`, and `/config` shows the last published effective snapshot plus current source state. Page mutations check their configuration revision and reject stale tabs.

The default MCP prompt contains every enabled server/tool description and complete input contract, plus the required `queries[].arguments` call envelope. `MCPTool action:"list"` remains a bounded inspection surface for server instructions and tool descriptions; `action:"describe"` returns one selected exact JSON schema. Calls validate against the same exact enabled catalog. `skill type:"load", action:"list"` returns effective skill metadata and exact source IDs; `action:"load"` reads the selected instructions.

List results use at most 50 rows plus a character budget. A partial result provides a runnable `next` tool call, including row/field position and `catalogRevision`. Copy it unchanged, including filters. Long descriptions and server instructions continue as fragments; a changed catalog returns a diagnostic and a restart call. Oversized skill identity metadata returns an explicit terminal limit instead of silently omitting the entry. A bounded skill load separately exposes continuations for remaining instruction text and supporting files.

Sources: [prompt preparation](../src/tools/prompt-capabilities.ts), [capability snapshots](../src/tools/capability-session.ts), [MCP pages](../src/tools/mcp/catalog-pages.ts), and [skill pages](../src/tools/skill-pages.ts).

## Worker grants

Workers receive explicit selections from the parent's enabled snapshot: native tool names, exact skill IDs, and MCP `{server, tool}` pairs. Spawn accepts `capabilities` with `snapshotRevision`; typed defaults are intersected with the parent's available capabilities. An explicitly empty selection grants no entries in that category. Custom workers require a non-empty role prompt and use top-level `tools[]` as their only native-tool selector; their optional `capabilities` object accepts only `skills` and `mcpTools`.

The parent changes a running worker through `agent type:"configure"`, providing `agentId`, the current `snapshotRevision`, replacement `capabilities` arrays, and optionally the expected `grantRevision`. Workers request missing access from their parent. They cannot grant themselves a disabled or unselected capability. `/config` displays Worker grants for inspection.

Grant removals take effect immediately. Additions apply before the worker's next turn. Parent disablement removes access even if an older grant included the identity. The worker's prompt, skill loader, active native tools, and MCP catalog use the same grant. MCP calls pass through the parent-owned broker, which validates current identity and grant state before dispatch; workers do not independently connect discovered MCP sources.

The [shared capability schemas](../../octocode-agent-contracts/src/capabilities.ts), [worker runtime](../src/tools/worker-capabilities.ts), and [MCP broker](../src/tools/mcp/broker.ts) own these boundaries. A skill's frontmatter `allowed-tools` field describes intended usage; it does not grant runtime access.

## Model definitions

Model files use a `providers` object with Pi-compatible provider fields and model definitions. A minimal local provider example is:

```json
{
  "providers": {
    "local-example": {
      "api": "openai-completions",
      "baseUrl": "http://localhost:11434/v1",
      "apiKey": "local",
      "models": [{ "id": "example-model", "contextWindow": 32768 }]
    }
  }
}
```

The [model adapter](../src/adapters/pi-model-discovery.ts) merges providers and model IDs. Workspace definitions override matching global Octocode fields. A workspace model entry can change one field while retaining the other fields from its global entry. Pi's built-in models and sibling models defined in Pi configuration remain available.

Provider fields include `name`, `api`, `baseUrl`, `apiKey`, `headers`, and `authHeader`. Model fields include `id`, `name`, `api`, `baseUrl`, `reasoning`, `input`, `cost`, `contextWindow`, `maxTokens`, `headers`, `compat`, `thinkingLevelMap`, and `samplingParams`. Model IDs must be unique within one provider definition. Token limits must be positive numbers; costs must be finite, nonnegative numbers.

`modelOverrides` maps model IDs to partial model definitions. Global Octocode overrides apply before workspace overrides. Pi 0.85.1 applies the `modelOverrides` in its own `models.json` after the extension's model contributions, so those explicit Pi overrides remain final.

Pi retains ownership of active authentication. Configure OAuth through Pi; the Octocode model JSON adapter rejects `oauth`, `streamSimple`, and `refreshModels` fields. API key expressions remain inert during discovery. The configuration page exposes provider IDs, model IDs, names, APIs, and token limits; it omits keys, request headers, and endpoint URLs.

When you remove a source, the adapter removes its contributions and restores the registrations it replaced. It checks registration identity before cleanup so that another extension's later registration remains intact. Parse failures, unavailable registration APIs, provider composition failures, untrusted workspace entries, and registrations shadowed by another extension have explicit catalog states or diagnostics.

Source files must be regular files no larger than 1 MiB. Workspace sources must remain inside the workspace. The adapter rejects symlinked model files.

The [real Pi model tests](../tests/pi-model-discovery.test.ts) exercise the pinned 0.85.1 runtime, merged model IDs, final native overrides, active authentication, sibling preservation, and cleanup.

## Declarative command hooks

Each native hook source is a JSON file containing event-to-command definitions. For example, `.agents/hooks/tool-guard.json` can contain:

```json
{
  "hooks": {
    "tool_call": [
      { "type": "command", "command": "sh ./scripts/check-tool.sh", "timeout": 10 }
    ]
  }
}
```

The command receives one JSON object on standard input. The working directory is the active workspace. Input includes `hook_event_name`, `cwd`, available session identity, and the original Pi event fields. Tool events also provide `tool_name`, `tool_input`, or `tool_response`; input events provide `prompt`.

The shared Codex matcher-group format also works: an event contains groups with an optional `matcher` regular expression and a `hooks` array of command handlers. Matchers inspect the tool name for tool events and the reason for lifecycle events.

| Pi event | Shared event |
|---|---|
| `tool_call` | Eligible `PreToolUse` and `PermissionRequest` groups |
| `tool_execution_end` | `PostToolUse` |
| `session_before_compact` | `PreCompact` |
| `session_compact` | `PostCompact` |
| `input` | `UserPromptSubmit` |
| `agent_settled` | `Stop` |
| `session_start` | `SessionStart`; workers also dispatch `SubagentStart` |
| `session_shutdown` | `SessionEnd`; workers also dispatch `SubagentStop` |

The adapter accepts native Pi event names and the shared event names. [Hook discovery](../src/adapters/pi-hook-discovery.ts) also imports Codex `hooks.json` and the `hooks` section of `config.toml` from the Codex user directory and workspace `.codex` directory.

### Review and precedence

Each source has a stable location ID and a normalized definition revision. Sources remain pending review until you review that exact revision. The [shared capability state](../../octocode-agent-contracts/src/capability-state.ts) stores the reviewed revision and enablement in the extension state database. Memory storage mode keeps review state in memory instead.

Changing the normalized definition requires another review. Disabling a reviewed source prevents execution. A workspace source never executes without workspace trust, even when its revision was previously reviewed. Discovery and catalog refresh never execute commands.

A workspace native hook file shadows a global native hook file with the same filename. The global file remains visible as shadowed. An invalid, disabled, or unreviewed workspace replacement does not cause the global command to run. Other sources execute in deterministic scope and filename order, followed by their declared group and handler order.

Native hook directories and files cannot be symlinks. Source files must be regular files no larger than 1 MiB, and workspace files must remain inside the workspace. JavaScript and TypeScript modules are not declarative hook sources. Unsupported events, non-command handlers, and asynchronous handlers appear as unsupported and do not execute.

### Decisions and cancellation

A command exit status of `0` continues unless its JSON output denies the operation. Exit status `2` blocks. JSON output can use `decision: "block"`, `decision: "deny"`, `block: true`, `continue: false`, or `hookSpecificOutput.permissionDecision: "deny"`. A string `reason` or `hookSpecificOutput.permissionDecisionReason` supplies the blocking reason.

Blocking a tool event returns Pi's tool denial. Blocking input marks it handled, and blocking pre-compaction cancels compaction. Observe-only events record the outcome without acquiring permission to cancel a completed action.

The shared parser defaults command timeouts to 600 seconds. `SessionEnd` defaults to 1 second and rejects values above 3 seconds. The `timeout` field uses seconds. Cancellation and timeouts terminate the command's process group on POSIX systems. Commands also stop when their combined standard output and standard error exceed 64 KiB. Failed, timed-out, or cancelled commands block tool, input, and pre-compaction operations.

The [hook runtime](../src/adapters/pi-hook-runtime.ts) refreshes definitions before dispatch, checks the reviewed revision, bounds its execution receipts, and registers only once with a hook composer. [Hook runtime tests](../tests/pi-hook-runtime.test.ts) run isolated shell fixtures for decisions, cancellation, timeout, review state, and duplicate-registration checks.

See the [package architecture](../ARCHITECTURE.md) for session lifecycle ownership and the [configuration reference](../../../docs/CONFIGURATION.md) for environment and storage settings.
