# Octocode settings control center

`/settings` is the browser-based inventory and configuration surface for the
running Pi extension. It combines the live slash-command registry, MCP
connections and tools, MCP discovery, agent prompt artifacts, skills, and
persisted enablement overrides in one loopback-only page.

This document describes the implemented behavior in
`src/tools/mcp-html.ts`. MCP protocol behavior remains owned by
[`docs/MCP.md`](https://github.com/bgauryy/octocode/blob/main/docs/MCP.md), and session initialization is
owned by [`RUNTIME_STATE.md`](https://github.com/bgauryy/octocode/blob/main/packages/octocode-pi-extension/docs/RUNTIME_STATE.md).

## Entry points

| Command | Opens |
|---|---|
| `/settings` | `settings.html#skills` by default. |
| `/settings commands` | Live slash-command inventory. |
| `/settings skills` | Discovered skills and enablement. |
| `/settings connections` | MCP servers and tools. |
| `/settings add-server` | Managed MCP server editor. |
| `/settings sources` | MCP discovery sources and warnings. |
| `/settings agent-context` | MCP prompt mode and artifact status. |
| `/settings overrides` | Effective SQLite overrides for the workspace. |
| `/mcp` | Focused alias for `settings.html#connections`. |

The consolidated terminal footer intentionally omits keyboard-help clutter and
shows `/settings` as a one-tap cue. Running `/settings` regenerates the
HTML and snapshots the current live command registry before opening it.

## Page overview

The hero reports five current counts:

- live public slash commands;
- enabled MCP servers;
- discovered foreign MCP imports;
- tools in the cached MCP catalog;
- enabled skills versus all discovered skills.

The left navigation links to every section. On narrow screens it becomes a
horizontal scrolling navigation bar; cards, filters, statistics, and forms
collapse to one or two columns through the shared Octocode HTML theme.

## Commands

The Commands section is built from `pi.getCommands()` each time the page opens.
It is the same normalized registry used by `/commands`.

Implemented behavior:

- trims names, removes blank entries, deduplicates by command name, and sorts
  alphabetically;
- excludes internal commands whose names begin with `_`;
- includes extension, skill, and prompt commands registered in the running
  session;
- shows `/name`, description, source type, registration path, source package,
  scope, and origin;
- searches names, descriptions, source types, and registration sources;
- filters by All, Extension, Skills, or Prompts.

This is a read-only runtime inventory. The page does not execute commands or
enable/disable them. Re-run `/settings` after installing or registering a new
command to rebuild the snapshot.

## MCP connections and tools

The Connections section includes enabled, disabled, managed, built-in, and
read-only imported definitions. Server cards show:

- managed or discovered origin and the source host;
- connected/offline and enabled/disabled badges;
- stdio or Streamable HTTP transport;
- cached tool count;
- OAuth status: not required, authorized, or authorization required;
- effective scope, owning source path, and a redacted configuration summary;
- each cached tool's name, description, and effective enablement.

Server controls:

| Control | Behavior |
|---|---|
| Edit | Loads a managed definition into the transport-aware editor. Imported definitions remain read-only. |
| Enable / Disable | Writes a workspace or global SQLite override, stops any old connection, invalidates the server/workspace catalog, and starts a background refresh. |
| Enable import | Explicitly authorizes a namespaced foreign definition; imports are disabled by default. |
| Connect / retry | Restarts the connection, rediscovers the catalog, and initiates OAuth when required. |
| Remove | Removes a managed definition from canonical JSON, closes the connection, revokes stored OAuth credentials when applicable, and refreshes artifacts. |
| Tool Enable / Disable | Writes a per-tool SQLite override and refreshes the server catalog. |

The built-in `octocode` server cannot be removed, but a managed definition may
override it. Foreign definitions cannot be edited or removed from their owning
application's file.

Server search matches name, description, and source label. The Discovered
filter limits the view to imported definitions. A server without a cached tool
catalog explains that it must be enabled and connected before discovery.

## Add or edit a managed MCP server

The editor writes canonical Octocode JSON and refreshes the affected connection
and catalog. It supports:

- project or global scope;
- stdio or Streamable HTTP transport;
- server name, description, and timeout from 1,000 to 120,000 ms;
- stdio command, one argument per line, and working directory;
- HTTP(S) URL;
- environment references as `destination key -> environment variable name`;
- HTTP header references as `header name -> environment variable name`;
- HTTP authentication mode: references/none or OAuth.

Transport-specific fields appear only when relevant. Save shows progress,
posts one typed action, regenerates the page, and reloads on success. Mutation
errors appear in the page without losing the current form state.

Canonical definition files:

| Scope | Source of truth |
|---|---|
| Workspace | `$OCTOCODE_HOME/extension/workspaces/<workspace-key>/mcp/servers.json` |
| Global | `$OCTOCODE_HOME/extension/mcp/servers.json` |

Project writes require a trusted workspace. Definitions are validated and
written atomically while preserving the supported JSON container shape.

## Discovery sources

Octocode owns only its two canonical files. The page also discovers compatible
definitions from other hosts and presents them as namespaced, read-only,
disabled-by-default imports.

Project locations:

- `.mcp.json` and `.claude/mcp.json`;
- `.cursor/mcp.json`;
- `.codex/config.toml`;
- `.agents/mcp_config.json` and `.agents/mcp.json`;
- `.agent/mcp_config.json` and `.agent/mcp.json`;
- `.vscode/mcp.json`.

User locations:

- `~/.claude.json` and `~/.claude/mcp.json`;
- `~/.cursor/mcp.json`;
- `~/.codex/config.toml`;
- `~/.agents/mcp_config.json` and `~/.agents/mcp.json`;
- `~/.agent/mcp_config.json` and `~/.agent/mcp.json`;
- `~/.gemini/config/mcp_config.json`;
- `~/.gemini/antigravity/mcp_config.json`;
- `~/.gemini/antigravity-cli/mcp_config.json`;
- Claude Desktop's macOS and XDG configuration locations;
- `~/.vscode/mcp.json`.

The Discovery section shows the host, exact source path, trust status,
read-only/active classification, and parse/import warnings. Untrusted project
sources are not imported. Name collisions are resolved with host and scope
namespacing rather than silently overwriting another definition.

## Agent context and prompt artifacts

The Agent context section explains what MCP routing data the next agent call
will receive. It shows:

- exact or compact mode;
- prompt readiness (`pending`, `ready`, `frozen`, `stale`, or degraded state as
  reported by the runtime);
- injected MCP prompt character count;
- `mcp.md` availability and capture time;
- exact `catalog.json` and compact `mcp.md` paths when present;
- a `/new` warning when the current session prompt is frozen and stale.

Mode behavior:

| `OCTOCODE_COMPACT_MCP` | Agent prompt |
|---|---|
| Unset/enabled | Token-efficient deterministic `mcp.md`; exact `catalog.json` remains private for validation. |
| `0`/disabled | Exact enabled server instructions, tool descriptions, and normalized input schemas from `catalog.json`. |

Set `OCTOCODE_MCP_AI_GUIDE=1` to opt into a model-authored compact guide. The
default avoids the extra model request and uses the deterministic schema-aware index.

Artifacts live under
`$OCTOCODE_HOME/extension/mcp/workspaces/<workspace-key>/`. MCP and skill changes
take effect in runtime routing immediately, but a system prompt already frozen
for the session remains byte-stable. Start `/new` to expose the refreshed
catalog or skill list to the model.

## Skills

The Skills section deliberately shows the complete inventory, including
disabled skills that the agent cannot currently load. It supports text search
and All, Enabled, and Disabled filters.

Each skill card shows:

- source, name, description, and `SKILL.md` path;
- enabled/disabled state;
- whether the effective value came from a workspace override, global override,
  or the default;
- a This workspace / All workspaces scope selector;
- an Enable skill / Disable skill control.

Discovery merges Pi-provided metadata, bundled skills, and `SKILL.md` files
from these roots, in precedence order:

- project `.agent/skills`, `.agents/skills`, `.claude/skills`, `.cursor/skills`,
  `.codex/skills`, `.octocode/skills`, `.pi/agent/skills`, and `.pi/skills`;
- user `~/.agent/skills`, `~/.agents/skills`, `~/.pi/agent/skills`, `~/.pi/skills`, `~/.claude/skills`,
  `~/.cursor/skills`, `~/.codex/skills`, and `~/.octocode/skills`;
- extension-bundled skills.

Disabled skills disappear immediately from the effective skill loader,
autocomplete, discovery inventory, dashboards, and generated agent skill
catalog. If the agent prompt is already frozen, `/new` is still required to
remove or add its prompt entry.

## Overrides and persistence

The Overrides section exposes normalized state for diagnosis; it never becomes
a second definition store.

| Data | Authoritative store |
|---|---|
| Managed MCP definitions | Canonical project/global JSON files. |
| Foreign MCP definitions | Their owning host files; Octocode imports them read-only. |
| Server/tool enablement | Shared Octocode SQLite `mcp_server_overrides` and `mcp_tool_overrides`. |
| Skill enablement | Shared Octocode SQLite `skill_overrides`. |
| OAuth access/refresh tokens | OS credential store. |
| Exact schemas/instructions | Workspace `catalog.json`. |
| Compact guide | Workspace `mcp.md`. |
| Generated control-center page | `$OCTOCODE_HOME/tmp/mcp/<workspace-digest>/settings.html`. |

Precedence:

- skills: workspace override -> global override -> enabled by default;
- servers: workspace override -> global override -> definition default;
- tools: workspace tool -> workspace server -> global tool -> global server ->
  definition default.

The page does not duplicate definitions, schemas, health, or credentials in
SQLite. If SQLite diagnostics are unavailable, the page remains readable and
uses safe defaults, but override details may be absent.

## Security model

The settings page is a local privileged surface, not a public web application.
Protections include:

- one process-shared HTTP server bound only to `127.0.0.1` on an ephemeral port;
- an exact loopback Host allowlist to resist DNS rebinding;
- same-origin checks on mutation requests;
- an unguessable 32-byte per-page action token sent in
  `x-octocode-action-token`;
- POST-only JSON mutation endpoints with a 16 KiB body ceiling;
- strict action, scope, server/tool/skill name, configuration-field, environment
  name, and HTTP header-name validation;
- project-trust enforcement before project definition or skill mutations;
- lexical and realpath/symlink containment checks for every served file;
- `Cache-Control: no-store` and `X-Content-Type-Options: nosniff`;
- HTML escaping for commands, descriptions, paths, MCP metadata, skills, and
  diagnostics;
- URL redaction of user info, query, and fragment;
- display of environment/header keys and references rather than their values;
- OAuth tokens kept in the OS credential store and never rendered.

The form rejects raw `env` and `headers` maps. Arguments and descriptions are
configuration text and remain visible for managed definitions, so credentials
must never be placed in them; use environment/header references or OAuth.
Imported stdio arguments are summarized by count rather than rendered.

## Refresh and lifecycle behavior

After a successful MCP mutation, Octocode stops the affected connection,
invalidates server/workspace caches, marks a frozen prompt stale, warms the MCP
catalog in the background, regenerates `settings.html`, and reloads the page.
Connect/retry performs a real reconnect before refresh. Removing an OAuth
server also attempts credential revocation.

Skill mutations update SQLite immediately, mark a frozen prompt stale, announce
the change through the unified runtime store, regenerate the page, and reload.

The shared local server is lazy, reused by other Octocode HTML surfaces,
`unref`'d so it cannot keep the process alive, and disposed with the session.

## Current boundaries

- Commands are a live read-only inventory; command execution remains in Pi.
- Enablement controls apply to MCP servers/tools and skills, not arbitrary Pi
  commands or direct provider tools.
- The Connections section shows the cached MCP tool catalog, not MCP resources,
  resource templates, or prompts; those remain available through `MCPTool`.
- Experimental MCP drafts and legacy SSE are outside the supported stable
  surface.
- A page already open does not poll for newly registered commands. Run
  `/settings` again for a new live registry snapshot.
- `/new` is required whenever a frozen system prompt must reflect changed MCP
  routing or skill metadata.
