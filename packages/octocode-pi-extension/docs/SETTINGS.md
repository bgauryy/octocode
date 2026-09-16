# Octocode configuration

`/config` opens the running Pi extension's configuration page in the OS browser.
`/configuration` is an alias. The loopback page brings together commands, MCP
connections and tools, skill sources, models, declarative hooks, worker grants,
prompt artifacts, and session controls.

Changes take effect on the next turn and bind the exact source ID. Worker grants
remain explicit and reviewable.

The implementation is in [`mcp/html.ts`](../src/tools/mcp/html.ts).
[Capability sources](CAPABILITIES.md) owns source paths, precedence, review,
adapter contracts, and worker grants. [Runtime state](RUNTIME_STATE.md) owns
session initialization and disposal.

## Entry points

| Command | Opens |
|---|---|
| `/config` | Configuration in the OS browser, starting at the overview. |
| `/configuration` | The same page through an alias. |

The footer shows `/config`. Running either command regenerates the HTML,
snapshots the live registry, and opens the browser. If the browser cannot open,
the notification includes a URL for manual opening.

Runtime controls change footer density and permissions immediately. Appearance
controls change theme and effort when the host supports them. Effort sets
thinking depth and worker concurrency. These controls apply to the current
session; unavailable host APIs leave their controls disabled. Review plan opens
the current plan with explicit Start and Request changes actions.

## Page overview

The overview reports public commands, enabled MCP servers, foreign imports,
cached MCP tools, and enabled versus discovered skills. Section links lead to
runtime, appearance, hooks, commands, MCP, source discovery, agent context,
models, skills, and override state. The layout adapts to narrow screens.

## Commands

The Commands section snapshots `pi.getCommands()` each time the page opens.
The extension contributes five entries: `/config`, `/configuration`,
`/octocode-rewind`, `/octocode-inbox`, and `/octocode-status`.
Host-provided commands, prompts, and skill commands remain in the inventory.

The page trims and deduplicates names, hides internal names beginning with `_`,
and sorts entries alphabetically. Each entry shows its description and available
registration source, package, scope, and origin. Search and source filters narrow
the list. This inventory is read-only; run commands in Pi. Reopen `/config` after
registering a new command to refresh the snapshot.

## MCP connections and tools

Connections include managed, built-in, imported, enabled, and disabled servers.
Cards show source path and scope, transport, connection and OAuth status, cached
tool count, redacted configuration, user/server instructions, tool descriptions,
exact input schemas, and effective tool enablement.

| Control | Behavior |
|---|---|
| Edit | Loads a managed definition into the transport-aware editor. |
| Review and link import | Reviews the current source ID and definition revision in the selected scope. The foreign file remains authoritative. |
| Enable / Disable | Changes server enablement. Enable import requires a valid reviewed revision and cannot bypass pending review. |
| Connect / retry | Restarts the connection and catalog discovery; starts OAuth when needed. |
| Remove | Removes a managed definition, closes its connection, and attempts OAuth credential revocation when applicable. |
| Tool Enable / Disable | Changes a per-tool override, subject to server enablement and source tool filters. |

Imported definitions cannot be edited or removed from their owning application's
file through this page. The built-in `octocode` server cannot be removed; a
managed definition can override it. Servers need an enabled connection before
their tools can be discovered. Search matches name, description, and source;
the Discovered filter selects imported definitions.

## Add or edit a managed MCP server

The editor supports project/global scope, stdio or Streamable HTTP transport,
server name, description, optional server instructions, and a timeout from
1,000 to 120,000 ms. Stdio fields include command, one argument per line, and
working directory. HTTP fields include URL and authentication through references
or OAuth. Environment and header references map destination keys to environment
variable names.

Transport fields appear only when relevant. Save validates one typed action,
writes atomically, refreshes the connection/catalog, and reloads the page.
Errors leave the form available for correction.

| Scope | Definition file |
|---|---|
| Workspace | `.agents/mcp.json` |
| Global | `$OCTOCODE_HOME/mcp.json` (default `~/.octocode/mcp.json`) |

Workspace writes require trust. Writes preserve supported JSON container shapes.
Direct JSON definitions also support startup timeouts and tool filters; the
browser editor exposes its supported subset.

## Discovery sources

Native sources are active by default, subject to definition enablement and
workspace trust. Global sources use `OCTOCODE_HOME`, and workspace sources use
`.agents/`. Pi defaults are used only for models: `~/.pi/agent/models.json`,
relocated by `PI_CODING_AGENT_DIR`. Bundled skills and explicit skill files outside
Pi default directories remain supported. Existing private Octocode
MCP paths remain lower-precedence inputs. The [source path table](CAPABILITIES.md#source-paths)
lists native locations and precedence.

Claude, Codex, Cursor, and other recognized foreign MCP/skill files appear as
disabled candidates. Discovery includes `.mcp.json`, `.cursor/mcp.json`,
`$CODEX_HOME/config.toml`, and Claude's user-file project entries. Discovery does
not execute their code, connect their servers, or copy their configuration.

The page shows the host, exact path, trust state, review status, and parse or
unsupported-field diagnostics. Review binds a source ID and definition revision
to project or global scope. Changed linked definitions become pending review;
removed reviewed definitions become unavailable. Namespaced MCP display names
avoid collisions, while source IDs retain exact identity.

## Agent context and prompt artifacts

The Agent context section shows catalog readiness, prompt character count, the exact
catalog path, and the last published effective capability revision. Parent capabilities lists enabled native tools, exact skill identities,
and MCP server/tool pairs. Worker grants shows each worker's selected access.

There is one deterministic prompt projection: a bounded `<mcp_catalog_index>` containing
enabled server instructions plus tool names and descriptions. Exact input schemas are not
injected. The exact `catalog.json` artifact lives under
`$OCTOCODE_HOME/extension/mcp/workspaces/<workspace-key>/`.

The runtime resolves effective capabilities before every turn. Changes appear in
the next turn's prompt/catalog without starting a new session. An unchanged
projection stays byte-stable. A `stale` badge means a changed source is awaiting
the next projection; it does not require `/new`.

The prompt supplies enabled MCP routing metadata only. `MCPTool action:"describe"` returns
the selected exact schema and activates a directly callable namespaced Pi proxy when the host
admits dynamic names; otherwise it reports the generic gateway fallback. Generic gateway calls
are blocked until that schema is described. The inspection surface stays
bounded: `action:"list"` returns instructions and descriptions with
revision-bound executable continuations.
Skill list → load follows the same staged discovery pattern. Copy a partial
result's executable `next` call unchanged; it carries the catalog revision and
any field-fragment position. [Catalog contracts](CAPABILITIES.md#versioned-prompt-and-catalogs)
describe continuation and revision failures.

## Skills

The Skills section shows effective names and all their source alternatives,
including disabled, shadowed, pending-review, unavailable, invalid, and untrusted
candidates. Search and All/Enabled/Disabled filters narrow the inventory. Cards
show name, description, `SKILL.md` path, source, revision, selection, and effective
enablement. A scope selector chooses This workspace or All workspaces.

Review and select source approves the exact file revision and selects its source
ID. Bundled skills win by default; an explicit selection can override that
collision. Ordinary workspace skills outrank global skills, with the nearest
repository ancestor first. Pi metadata must resolve to a valid skill file. A changed
or removed selected source stays selected and blocks fallback; review its current
revision or explicitly select another source to restore access.

Enable skill and Disable skill change name enablement. They do not authorize
an unreviewed foreign source. Disabled or changed sources disappear from the
effective loader and the next turn's prompt inventory, while remaining visible
for review. Recursive and linked skill directories use realpath cycle and change
checks. See [effective skills](CAPABILITIES.md#effective-skills).

## Models and hooks

The Models section shows provider/model metadata and discovery diagnostics.
Global `$OCTOCODE_HOME/models.json` and workspace `.agents/models.json` contribute
to Pi's registry while retaining built-in models, active authentication, native
Pi configuration, and other extension registrations. Credentials, headers, and
endpoint URLs are omitted from the page. Edit the owning source file to change
model definitions.

The Hooks section shows declarative command sources, event names, exact revisions,
status, and execution health. Review definition approves the current definition;
Enable and Disable control reviewed execution. Workspace commands also require
current trust. Changing a normalized hook definition requires another review.
Discovery never executes commands. [Model and hook adapters](CAPABILITIES.md#model-definitions)
documents field validation, event mapping, decisions, timeout, and cancellation.

## Overrides and persistence

The Overrides section exposes state for diagnosis.

| Data | Authoritative store |
|---|---|
| Managed MCP definitions | Native project/global JSON files. |
| Foreign MCP and skill definitions | Original files, read through reviewed links. |
| Server/tool enablement | Extension SQLite `mcp_server_overrides` and `mcp_tool_overrides`. |
| Skill name enablement | Extension SQLite `skill_overrides`. |
| Source reviews and selections | Extension-owned capability state, keyed by scope and source ID. |
| OAuth access/refresh tokens | OS credential store. |
| Exact MCP schemas/instructions | Workspace `catalog.json`. |
| Generated configuration page | `$OCTOCODE_HOME/extension/tmp/settings/<workspace-digest>/settings.html`. |

Workspace overrides precede global overrides. Tool enablement considers the
workspace tool/server overrides before global tool/server defaults. Source
review, supported definitions, workspace trust, and source tool filters are
additional gates. Memory storage mode keeps runtime review and enablement state
in memory. If the state store is unavailable, the page explains the failure and
disables controls that cannot save their changes.

## Security model

The page uses one process-shared HTTP server bound to `127.0.0.1` on an ephemeral
port. It checks the exact loopback Host, mutation origin, POST method, JSON body
limit of 32 KiB, and an unguessable 32-byte per-page token carried in
`x-octocode-action-token`.

Actions validate names, scopes, configuration fields, environment/header
references, current source revisions, and project trust. Served files pass lexical
and realpath containment checks. Responses use `Cache-Control: no-store` and
`X-Content-Type-Options: nosniff`. Dynamic HTML is escaped. URLs redact user info,
query, and fragment; environment/header values and OAuth tokens are not rendered.

The form rejects raw `env` and `headers` maps. Arguments and descriptions remain
visible for managed definitions, so use references or OAuth for credentials.
Imported stdio arguments are summarized by count.

## Refresh and lifecycle behavior

MCP mutations stop affected connections, invalidate caches, and refresh the
catalog and page. Connect/retry performs a reconnect. Skill mutations update
state and regenerate the page. Adapter refresh reads current model and hook
sources. The effective prompt/catalog projection updates on the next turn;
worker removals also apply immediately at their execution boundary.

The local server is lazy, reused by other HTML surfaces, and `unref`'d so it
cannot keep the process alive. Session shutdown retires configuration mounts
and cached controls. Reopening rotates the action token. Actions are serialized
and stale configuration revisions are rejected with an instruction to reopen
`/config` and review current definitions.

## Current boundaries

- Commands and model definitions are inventories; run commands in Pi and edit
  model definitions in their source files.
- Enablement and review controls cover MCP, skills, and declarative hooks.
  They do not grant worker access; parents use `agent type:"configure"`.
- Connections lists cached tools. MCP resources, templates, and prompts remain
  available through `MCPTool`.
- Unsupported foreign fields and legacy SSE are reported instead of activated.
- Open pages do not poll for new commands or source changes. Reopen `/config`
  for a fresh snapshot; prompt changes apply at the next turn boundary.
