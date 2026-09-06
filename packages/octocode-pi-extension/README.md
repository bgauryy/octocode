# `@octocodeai/pi-extension`

The official Octocode package for Pi. It combines Octocode research through MCP with guarded file and shell operations, subagents, skills, media workflows, planning, and a live settings interface.

The Pi host SDK (`@earendil-works/pi-coding-agent` 0.84.4) is a required peer dependency because the extension imports its runtime APIs.

## Install

Requires Node.js 22.22.2+ (22.x), 24.15.0+ (24.x), or 26+.

```bash
pi install npm:@octocodeai/pi-extension
```

The package registers its extension and themes through Pi's package manifest. Octocode's built-in MCP server is configured automatically, preferring the package-local server and falling back to `npx -y octocode-mcp@latest`.

## Runtime surface

The live source inventory is authoritative. Use `/octocode-harness` inside Pi for the runtime view and see [HARNESS.md](HARNESS.md) for the detailed contract.

| Surface | Count |
|---|---:|
| Native Octocode research tools | 0 (research is served through `MCPTool`) |
| Pi support tools | 16 |
| Guarded Pi builtin overrides | 1 (`bash`) |
| Disabled Pi builtins | 6 |
| Slash command entries | 24 |
| Bundled main-agent skills | 14 |

### Support tools

| Tool | Purpose |
|---|---|
| `file` | Create, edit, or delete files through one guarded mutation boundary. |
| `web` | Fetch an absolute URL or search the web. |
| `chromeDebug` | Inspect and control Chrome through CDP. |
| `agent` | Spawn and manage researcher, architect, planner, browser, and custom workers. |
| `callTool` | Invoke a capability from the live dynamic-tool registry. |
| `skill` | Load and manage installed skills. |
| `plan` | Manage session and shared plans with verification receipts. |
| `localServer` | Serve an inspected local directory on loopback for review. |
| `MCPTool` | Discover, describe, call, and manage MCP tools and servers. |
| `askUser` | Request structured input through Pi's UI. |
| `memory` | Recall and manage Awareness memory when persistent storage is enabled. |
| `lock` | Coordinate exceptional exclusive file access. |
| `message` | Exchange small cross-agent coordination messages. |
| `readMedia` | Inspect images, video, and audio. |
| `media` | Create or transform media and PDFs. |
| `runFfmpeg` | Run guarded ffmpeg or ffprobe argument lists. |

The extension overrides `bash` with command and path guards. It removes Pi's public `read`, `edit`, `write`, `grep`, `find`, and `ls` tools; use Octocode research tools for reads and discovery, and `file` for mutations.

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

See the repository [configuration guide](https://github.com/bgauryy/octocode/blob/main/docs/CONFIGURATION.md) for every supported key and [docs/SETTINGS.md](docs/SETTINGS.md) for Pi's control center, persistence, and security behavior.

## Slash command entries (24)

| Command | Purpose |
|---|---|
| `/commands` | List the live command inventory. |
| `/octocode` | Open the Octocode dashboard. |
| `/octocode-harness` | Show the complete registered surface. |
| `/octocode-now` | Show current work state. |
| `/octocode-tasks` | List coordinated tasks. |
| `/octocode-skills` | Inspect skill readiness. |
| `/octocode-agents` | Manage worker agents. |
| `/octocode-cron` | Manage session jobs. |
| `/settings` | Open the complete settings control center. |
| `/mcp` | Open MCP connections and tools. |
| `/octocode-setup` | Install Octocode prompt integration. |
| `/octocode-skills-update` | Update package skills and reload resources. |
| `/octocode-plan` | Show or manage the active plan. |
| `/octocode-theme` | Inspect or change theme behavior. |
| `/octocode-chrome` | Inspect Chrome integration. |
| `/octocode-footer` | Configure footer details. |
| `/octocode-permissions` | Inspect permission state. |
| `/octocode-profile` | Inspect or choose an agent profile. |
| `/octocode-inbox` | Open coordination messages. |
| `/octocode-palette` | Open the Octocode command palette. |
| `/octocode-rewind` | Rewind supported session state. |
| `/octocode-dial` | Adjust reasoning effort. |
| `/octocode-watch` | Manage watch behavior. |
| `/octocode-export` | Export supported session artifacts. |

Command arguments and lifecycle details are documented in [HARNESS.md](HARNESS.md).

## Bundled skills (14)

The build copies these main-agent skills into `dist/skills/`:

- `octocode-architect`
- `octocode-brainstorming`
- `octocode-chrome-devtools`
- `octocode-code-graph`
- `octocode-documentation`
- `octocode-eval-benchmark`
- `octocode-orchestrator`
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

The package's production test suite checks the documented inventories against the executable harness so counts and names cannot drift silently.
