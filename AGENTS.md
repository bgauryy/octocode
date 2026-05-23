# AGENTS.md — Octocode Monorepo

> Single source of AI agent guidance for the Octocode monorepo. Covers the root and every package — there is **no** per-package `AGENTS.md`.

## Contents

**Monorepo**
- [Documentation Links Rule](#documentation-links-rule)
- [Core Methodology](#core-methodology)
- [Repository Structure](#repository-structure)
- [Access Control](#access-control-monorepo-wide)
- [Quick Commands](#quick-commands)
- [Key References](#key-references)

**Packages**
- [`octocode-mcp`](#package-octocode-mcp) — MCP server (14 tools)
- [`octocode-cli`](#package-octocode-cli) — CLI installer + tool runner
- [`octocode-shared`](#package-octocode-shared) — Credentials, sessions, platform
- [`octocode-vscode`](#package-octocode-vscode) — VS Code extension
- [`octocode-security-utils`](#package-octocode-security-utils) — Security utilities

---

## Documentation Links Rule

All links in documentation files (`docs/`, package READMEs) **MUST** use absolute GitHub URLs — never relative paths.

**Base URL:** `https://github.com/bgauryy/octocode-mcp/blob/main/`

```
❌ WRONG: Config -> ./CONFIGURATION_REFERENCE.md
❌ WRONG: Auth -> ../docs/AUTHENTICATION_SETUP.md
✅ RIGHT: [Config](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/CONFIGURATION_REFERENCE.md)
✅ RIGHT: [Auth](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/providers/AUTHENTICATION_SETUP.md)
```

## Core Methodology

1. **Task Management**: Review → Plan (use `todo` tool) → Track progress
2. **Research**: Prefer `octocode-local` MCP tools. LSP first, then local search, then GitHub
3. **TDD**: Write failing test → Run (`yarn test`) → Fix → Verify coverage (90%)
4. **ReAct Loop**: Reason → Act → Observe → Loop
5. **Quality**: Clean Code, run `yarn lint` + `yarn test`, use `npx knip` for dead code
6. **Efficiency**: Use Linux commands (`mv`, `cp`, `sed`) for file operations

> **File Operations**: Use Linux commands for file changes and prefer batching changes.
> For command examples and workflows, see: [Linux & File Operations](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/DEVELOPMENT_GUIDE.md#linux--file-operations)

## Repository Structure

```
octocode-mcp/
├── packages/
│   ├── octocode-mcp/             # MCP server: GitHub/GitLab/Bitbucket, local tools, LSP
│   ├── octocode-cli/             # CLI installer, tool runner, skills marketplace
│   ├── octocode-vscode/          # VS Code extension (OAuth, multi-editor MCP install)
│   ├── octocode-shared/          # Shared utilities (credentials, platform, session)
│   └── octocode-security-utils/  # Standalone security utilities (no AGENTS section)
├── skills/                       # AI agent skills (research, plan, roast, etc.)
├── docs/                         # ALL monorepo documentation (provider setup, references, workflows)
└── package.json                  # Workspace root (yarn workspaces)
```

## Access Control (monorepo-wide)

| Path | Access |
|------|--------|
| `packages/*/src/`, `packages/*/tests/` | ✅ Auto |
| `docs/` | ✅ Auto |
| `*.json`, `*.config.*` | ⚠️ Ask |
| `.env*`, `.octocode/`, `node_modules/`, `dist/`, `out/`, `coverage/` | ❌ Never |

## Quick Commands

Canonical command list lives in the [Development Guide](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/DEVELOPMENT_GUIDE.md) (Commands & Workflow section).

## Key References

### Core
- **Docs Index**: [docs/README.md](https://github.com/bgauryy/octocode-mcp/blob/main/docs/README.md)
- **Configuration Docs**: [docs/configuration/](https://github.com/bgauryy/octocode-mcp/tree/main/docs/configuration) — install, auth providers, MCP clients, env/config, troubleshooting
- **Developer Docs**: [docs/dev/](https://github.com/bgauryy/octocode-mcp/tree/main/docs/dev) — tool/API references, workflows, architecture, contributing, skills
- **Specs**: [docs/specs/](https://github.com/bgauryy/octocode-mcp/tree/main/docs/specs) — design specs and RFCs
- **Development Guide**: [docs/dev/DEVELOPMENT_GUIDE.md](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/DEVELOPMENT_GUIDE.md)
- **Configuration**: [docs/configuration/CONFIGURATION_REFERENCE.md](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/CONFIGURATION_REFERENCE.md)
- **Troubleshooting**: [docs/configuration/TROUBLESHOOTING.md](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/TROUBLESHOOTING.md)

### Octocode MCP
- **GitHub/GitLab/Bitbucket Tools**: [docs/dev/reference/GITHUB_GITLAB_TOOLS_REFERENCE.md](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/GITHUB_GITLAB_TOOLS_REFERENCE.md)
- **Local + LSP Tools**: [docs/dev/reference/LOCAL_TOOLS_REFERENCE.md](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/LOCAL_TOOLS_REFERENCE.md)
- **Clone & Local Workflow**: [docs/dev/workflows/CLONE_AND_LOCAL_TOOLS_WORKFLOW.md](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/workflows/CLONE_AND_LOCAL_TOOLS_WORKFLOW.md)
- **Authentication**: [docs/configuration/providers/AUTHENTICATION_SETUP.md](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/providers/AUTHENTICATION_SETUP.md) · [GitHub](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/providers/GITHUB_SETUP_GUIDE.md) · [GitLab](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/providers/GITLAB_SETUP_GUIDE.md) · [Bitbucket](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/providers/BITBUCKET_SETUP_GUIDE.md)
- **Using with Pi**: [docs/configuration/clients/PI_SETUP_GUIDE.md](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/clients/PI_SETUP_GUIDE.md)

### Octocode CLI
- **CLI Reference**: [docs/dev/reference/CLI_REFERENCE.md](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/CLI_REFERENCE.md)
- **Skills Guide**: [docs/dev/SKILLS_GUIDE.md](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/SKILLS_GUIDE.md)
- **CLI vs MCP Benchmark**: [docs/dev/workflows/BENCHMARK.md](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/workflows/BENCHMARK.md)

### Octocode Shared
- **API Reference**: [docs/dev/reference/SHARED_API_REFERENCE.md](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/SHARED_API_REFERENCE.md)
- **Credentials**: [docs/dev/architecture/CREDENTIALS_ARCHITECTURE.md](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/architecture/CREDENTIALS_ARCHITECTURE.md)
- **Session Persistence**: [docs/dev/architecture/SESSION_PERSISTENCE.md](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/architecture/SESSION_PERSISTENCE.md)

### Skills
- **All Skills**: [skills/README.md](https://github.com/bgauryy/octocode-mcp/blob/main/skills/README.md)
- **Skills Guide**: [docs/dev/SKILLS_GUIDE.md](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/SKILLS_GUIDE.md)

---

# Package: `octocode-mcp`

MCP server for GitHub/GitLab/Bitbucket research, local code exploration, and LSP semantic navigation.

Run commands from `packages/octocode-mcp/`.

### Commands

| Task | Command |
|------|---------|
| Build | `yarn build` (lint + bundle with tsdown) |
| Build (dev) | `yarn build:dev` |
| Build (watch) | `yarn build:watch` |
| Clean | `yarn clean` |
| Test | `yarn test` (coverage) · `yarn test:full` · `yarn test:quiet` · `yarn test:watch` · `yarn test:ui` |
| Typecheck | `yarn typecheck` |
| Lint | `yarn lint` / `yarn lint:fix` |
| Format | `yarn format` / `yarn format:check` |
| Debug | `yarn debug` (MCP Inspector) |
| Binary builds | `yarn build:bin[:darwin-arm64|:darwin-x64|:linux-arm64|:linux-x64|:linux-x64-musl|:windows-x64|:all]` |

### Source layout

```
src/
├── index.ts, serverConfig.ts, session.ts, responses.ts, errorCodes.ts, types.ts, public.ts
├── hints/        # Dynamic + static hint generation
├── scheme/       # Shared schema utilities (baseSchema.ts)
├── tools/        # 14 tool modules, each: execution.ts, scheme.ts, types.ts, register.ts, index.ts
│                 # toolsManager.ts, toolRegistry.ts, toolConfig.ts, toolMetadata.ts, toolNames.ts
├── github/       # Octokit client, code/repo/PR/file search, query builders, errors
├── gitlab/       # GitLab API + provider implementation
├── providers/    # Multi-provider abstraction (github/gitlab/bitbucket) via factory
├── lsp/          # LSP client pool, server lifecycle, symbol resolution
├── security/     # withSecurityValidation, contentSanitizer, pathValidator, commandValidator,
│                 # 200+ secret regexes (ai-providers, cloud-infrastructure, auth-crypto, etc.)
├── commands/     # Builders: Ripgrep / Find / Ls (whitelist only)
├── utils/        # core/, credentials/, environment/, exec/, file/, http/, minifier/,
│                 # package/, pagination/, parsers/, response/
└── prompts/      # MCP prompt registration
```

```
tests/  ←  index.*, serverConfig.*, session.*, errorCodes,
          commands/, errors/, github/ (29), lsp/ (9), security/ (15),
          scheme/, hints/, tools/ (54), utils/ (37), integration/, helpers/, fixtures/
```

### Tools (14)

| Tool | Type | Local | Description |
|------|------|-------|-------------|
| `githubSearchCode` | search | ❌ | Search code across GitHub/GitLab |
| `githubGetFileContent` | content | ❌ | Fetch file or directory (`type:"directory"` needs `ENABLE_CLONE`) |
| `githubViewRepoStructure` | content | ❌ | Browse repo tree |
| `githubCloneRepo` | content | ✅ | Clone GitHub repos/subtrees for local + LSP analysis (`ENABLE_CLONE`) |
| `githubSearchRepositories` | search | ❌ | Search repositories |
| `githubSearchPullRequests` | history | ❌ | Search PRs/MRs and view diffs |
| `packageSearch` | search | ❌ | NPM/PyPI package + repo URL lookup |
| `localSearchCode` | search | ✅ | ripgrep search |
| `localViewStructure` | content | ✅ | Browse local directories |
| `localFindFiles` | search | ✅ | Find files by metadata |
| `localGetFileContent` | content | ✅ | Read local file content |
| `lspGotoDefinition` | LSP | ✅ | Jump to symbol definition |
| `lspFindReferences` | LSP | ✅ | Find all usages of a symbol |
| `lspCallHierarchy` | LSP | ✅ | Trace function call relationships |

LSP tools are standalone (no IDE required); TS/JS bundled, 30+ other langs via installed servers; cross-platform.

### Tool registration flow

```
Schema (Zod) → registerTool() → Security wrapper → Bulk handler → Implementation → Sanitizer → Response
```

Tools return `structuredContent` validated against `outputSchema`. Handles tracked in `toolRegistry.ts` (runtime `enable()`/`disable()`/`remove()`). Server advertises `listChanged: true`; background init deferred to `oninitialized`.

### Design rules

- **Modular tools** — self-contained directory per tool
- **Bulk queries** — every tool accepts 1–5 queries per request
- **Research context required** — every query needs `mainResearchGoal`, `researchGoal`, `reasoning`
- **Security first** — all I/O sanitized, secrets redacted, paths validated, command whitelist (`rg`, `find`, `ls`)
- **Single-engine search** — `localSearchCode` uses bundled `@vscode/ripgrep` only; no grep fallback
- **Pooled LSP clients** — acquire through `LspClientPool`; callers MUST NOT call `client.stop()`
- **Token efficiency** — minification, YAML default, response prioritization

### Environment variables

| Variable | Description | Default |
|----------|-------------|---------|
| `GITHUB_TOKEN` / `OCTOCODE_TOKEN` / `GH_TOKEN` | GitHub auth (priority: OCTOCODE > GH > GITHUB) | – |
| `GITHUB_API_URL` | GitHub API base URL | `https://api.github.com` |
| `GITLAB_TOKEN` / `GL_TOKEN` | GitLab auth | – |
| `GITLAB_HOST` | GitLab instance URL | `https://gitlab.com` |
| `ENABLE_LOCAL` | Enable local FS tools | `true` |
| `ENABLE_CLONE` | Enable `githubCloneRepo` + directory mode (requires `ENABLE_LOCAL`) | `false` |
| `OCTOCODE_CACHE_TTL_MS` | Clone cache TTL (ms) | `86400000` |
| `DISABLE_PROMPTS` | Disable prompts/slash commands | `false` |
| `LOG` | Enable session logging | `true` |
| `REQUEST_TIMEOUT` | API timeout (ms) | `30000` |
| `MAX_RETRIES` | Max retry attempts | `3` |
| `TOOLS_TO_RUN` / `ENABLE_TOOLS` / `DISABLE_TOOLS` | Comma-separated tool filters | – |
| `OCTOCODE_OUTPUT_FORMAT` | `yaml` (default) or `json` | `yaml` |

### Key files

| Purpose | File(s) |
|---------|---------|
| Entry point | `src/index.ts` |
| Tool registration | `src/tools/toolsManager.ts`, `src/tools/toolConfig.ts` |
| Tool registry | `src/tools/toolRegistry.ts` |
| Tool modules | `src/tools/<tool_name>/` (scheme.ts, execution.ts, types.ts) |
| Hints | `src/hints/` |
| Security wrapper | `src/security/withSecurityValidation.ts` |
| Secret detection | `src/security/contentSanitizer.ts`, `src/security/regexes/` |
| Path validation | `src/security/pathValidator.ts` |
| GitHub / GitLab clients | `src/github/client.ts`, `src/gitlab/client.ts` |
| Provider factory | `src/providers/factory.ts` |
| LSP client + config | `src/lsp/client.ts`, `src/lsp/config.ts`, `src/lsp/manager.ts` |
| Bulk operations | `src/utils/response/bulk.ts` |
| Package search | `src/utils/package/npm.ts`, `src/utils/package/python.ts` |

### Safety (package)

| Path | Access |
|------|--------|
| `src/`, `tests/` | ✅ FULL |
| `*.json`, `*.config.*` | ⚠️ ASK |
| `dist/`, `coverage/`, `node_modules/` | ❌ NEVER |

### Testing — 90% coverage required, Vitest + v8.

---

# Package: `octocode-cli`

CLI binary that **manages** Octocode (install, auth, skills, MCP marketplace, sync, cache) and **runs tools** (any Octocode tool from terminal).

Run commands from `packages/octocode-cli/`.

### Using the CLI

```
octocode-cli --help                          # all commands + all 14 tools
octocode-cli --tool <name> --help            # input/output schema for one tool
octocode-cli --tools-context                 # full MCP instructions + all schemas (~2200 lines)
octocode-cli --tool <name> --queries '<json>' [--json]
```

`--queries` accepts an object, array, or `{ "queries": [...] }`. Fields `id`, `researchGoal`, `reasoning`, `mainResearchGoal` are auto-filled — only provide tool-specific fields.

```bash
octocode-cli --tool localSearchCode --queries '{"path":".","pattern":"runCLI"}'
octocode-cli --tool githubSearchCode --queries '{"keywordsToSearch":["useReducer"],"owner":"facebook","repo":"react"}'
```

Output shape: `{ "content": [{ "type": "text", "text": "..." }], "structuredContent": {}, "isError": false }`

### Management commands

| Command | Aliases | Usage |
|---------|---------|-------|
| `install` | `i`, `setup` | `install --ide <client> [--method <npx\|direct>] [--force]` |
| `auth` | `a`, `gh` | `auth [login\|logout\|status\|token]` |
| `login` | `l` | `login [--hostname <host>] [--git-protocol <ssh\|https>]` |
| `logout` | – | `logout [--hostname <host>]` |
| `status` | `s` | `status [--hostname <host>]` |
| `token` | `t` | `token [--type <auto\|octocode-cli\|gh>] [--hostname <host>] [--source] [--json]` |
| `sync` | `sy` | `sync [--force] [--dry-run] [--status]` |
| `skills` | `sk` | `skills [install\|remove\|list] [--skill <name>] [--targets <list>] [--mode <copy\|symlink>] [--force]` |
| `mcp` | – | `mcp [list\|install\|remove\|status] [--id <id>] [--client <client>\|--config <path>] [--search <text>] [--category <name>] [--env K=V] [--installed] [--force]` |
| `cache` | – | `cache [status\|clean] [--repos] [--skills] [--logs] [--tools\|--local\|--lsp\|--api] [--all]` |

Supported clients: `cursor`, `claude-desktop`, `claude-code`, `windsurf`, `zed`, `vscode-cline`, `vscode-roo`, `vscode-continue`, `opencode`, `trae`, `antigravity`, `codex`, `gemini-cli`, `goose`, `kiro`.

### Dev commands

| Task | Command |
|------|---------|
| Build | `yarn build` (lint + bundle) · `yarn build:dev` |
| Test | `yarn test` (coverage) |
| Lint / Typecheck / Start | `yarn lint` · `yarn lint:fix` · `yarn typecheck` · `yarn start` |
| Validate registries | `yarn validate:mcp` · `yarn validate:skills` |

### Source layout

```
src/
├── index.ts, interactive.ts
├── cli/        # runCLI, parser, commands, tool-command, help, types
├── configs/    # mcp-registry (70+ MCPs), skills-marketplace
├── features/   # gh-auth, github-oauth, install, node-check, sync
├── types/, ui/, utils/  # colors, fs, mcp-config, mcp-io, mcp-paths,
                         # platform, shell, skills, token-storage, etc.
```

```
tests/  ←  cli/, configs/, features/, security/ (audit-findings, oauth-security),
           ui/ (external-mcp-flow), utils/
```

### Architecture

```
main() → runCLI() → [command handler] OR runInteractiveMode()
```

CLI args parse first; if a command or `--tool` matches, it runs. Otherwise falls through to interactive menu.

### Skills directory

Bundled skills live at repo root [`skills/`](https://github.com/bgauryy/octocode-mcp/tree/main/skills). At publish, `prepack` copies them into `packages/octocode-cli/skills`. Run `yarn validate:skills` after changes.

### Adding things

- **New command** — define in `commands.ts`, add help spec in `command-help-specs.ts`, test in `cli/commands.test.ts`.
- **New tool** — tools come from `octocode-mcp`; CLI auto-discovers them via MCP. No CLI changes needed.
- **New IDE** — add to `types/index.ts`, paths in `mcp-paths.ts`, install logic in `ui/install/`, add tests.
- **New skill** — create `skills/<name>/SKILL.md`, update `skills-marketplace.ts`, run `yarn validate:skills`.
- **New MCP server** — add entry in `mcp-registry.ts`, run `yarn validate:mcp`.

### Safety (package)

| Path | Access |
|------|--------|
| `src/`, `tests/` | ✅ FULL |
| `scripts/`, `*.json`, `*.config.*` | ⚠️ ASK |
| `out/`, `node_modules/` | ❌ NEVER |

Tokens encrypted in `~/.octocode/` (AES-256-GCM). Never log tokens. Coverage: 90% required.

---

# Package: `octocode-shared`

Shared utilities for credentials, session persistence, and platform detection across Octocode packages. Consumers: `octocode-cli`, `octocode-mcp`.

Run commands from `packages/octocode-shared/`.

### Commands

| Task | Command |
|------|---------|
| Build | `yarn build` (lint + tsc) · `yarn build:dev` · `yarn clean` |
| Test | `yarn test` (coverage) · `yarn test:quiet` · `yarn test:watch` |
| Lint / Typecheck | `yarn lint` · `yarn lint:fix` · `yarn typecheck` |

### Source layout

```
src/
├── index.ts                     # Package exports
├── credentials/
│   ├── index.ts
│   ├── credentialEncryption.ts  # AES-256-GCM encryption + file I/O
│   ├── storage.ts               # Encrypted storage
│   └── types.ts
├── platform/
│   ├── index.ts
│   └── platform.ts              # OS detection & paths
└── session/
    ├── index.ts
    ├── storage.ts               # Deferred-write session storage
    └── types.ts
```

### Module exports (entry points)

```ts
import { ... } from 'octocode-shared';
import { ... } from 'octocode-shared/credentials';
import { ... } from 'octocode-shared/platform';
import { ... } from 'octocode-shared/session';
```

Full export tables: see [`docs/SHARED_API_REFERENCE.md`](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/SHARED_API_REFERENCE.md).

### Credential storage

```
Token → AES-256-GCM → Base64 → ~/.octocode/credentials.json
Key:    ~/.octocode/.key  (file-based, restrictive perms)
```

- **AES-256-GCM** authenticated encryption, random IV per encryption
- **Token resolution chain**: env → encrypted storage → `gh` CLI fallback
- **Auto-refresh** for tokens with `refreshToken` (GitHub App tokens, 8h expiry); OAuth App tokens never expire
- **In-memory cache**: 5-min TTL, invalidated on `storeCredentials` / `deleteCredentials` / `updateToken` / `refreshAuthToken`

### Token resolution flow

```
resolveTokenFull(options)
    ↓
getTokenFromEnv()  → highest priority, NO refresh (user-managed)
    1. OCTOCODE_TOKEN  2. GH_TOKEN  3. GITHUB_TOKEN
    ↓
getTokenWithRefresh(host)  → ONLY octocode tokens are refreshed
    in-memory cache (5-min TTL) → file storage → auto-refresh via @octokit/oauth-methods
    ↓
getGhCliToken(host)  → fallback, gh manages its own refresh
```

| Token source | Auto-refresh? | Reason |
|---|---|---|
| Env vars | ❌ | User-managed |
| Octocode credentials | ✅ if `refreshToken` present | GitHub App tokens |
| `gh` CLI | ❌ | gh manages its own |

### Session storage

```
In-memory cache ↔ Deferred writes → ~/.octocode/session.json
Flush triggers: timer, explicit flush, SIGINT/SIGTERM, beforeExit
```

Tracks: `sessionId`, `createdAt`, `lastActiveAt`, `stats.{toolCalls, promptCalls, errors, rateLimits}`.

### Package guidelines

1. Minimal runtime dependencies (currently: `zod`, `@octokit/oauth-methods`, `@octokit/request`)
2. Cross-platform (macOS, Linux, Windows)
3. Type-safe exports — strict mode
4. Security-first — every credential operation encrypts
5. Performance — session writes deferred
6. Minimal API surface — export only what consumers need

### Safety (package)

| Path | Access |
|------|--------|
| `src/`, `tests/` | ✅ FULL |
| `*.json`, `*.config.*` | ⚠️ ASK |
| `dist/`, `coverage/`, `node_modules/` | ❌ NEVER |

Coverage: 90% required.

---

# Package: `octocode-vscode`

VS Code extension: GitHub OAuth, MCP server install across editors (Cursor, Windsurf, Antigravity, Trae, Cline, Roo Code), token sync.

Run commands from `packages/octocode-vscode/`.

### Commands

| Task | Command |
|------|---------|
| Build | `yarn build` (esbuild, minified) · `yarn watch` |
| Lint / Typecheck / Test | `yarn lint` · `yarn typecheck` · `yarn test` (Vitest) · `yarn test:quiet` |
| Verify | `yarn verify` (lint + typecheck + tests + build) |
| Package / Publish | `yarn package` (`.vsix`) · `yarn publish` (Marketplace) |

### Source layout

```
src/
├── extension.ts      # Extension activation + VS Code wiring
├── configPaths.ts    # Editor detection + MCP client config paths (pure, testable)
└── jsonUtils.ts      # Safe JSON file helper

tests/
├── configPaths.test.ts
└── jsonUtils.test.ts

images/icon.png
out/extension.js     # esbuild bundle
```

### Key components

| Component | Purpose |
|-----------|---------|
| `getEditorInfo()` | Detect current editor (Cursor, Windsurf, …) |
| `loginToGitHub()` | OAuth device flow |
| `syncTokenToAllConfigs()` | Push token to all detected MCP configs |
| `installMcpServer()` | Configure MCP server in editor config |
| `startMcpServer()` / `stopMcpServer()` | MCP server process lifecycle |
| `MCP_CLIENTS` | Registry of supported MCP clients |

### Extension commands

| Command ID | Description |
|---|---|
| `octocode.loginGitHub` / `logoutGitHub` / `showAuthStatus` | Auth lifecycle |
| `octocode.installMcp` / `startServer` / `stopServer` | MCP server |
| `octocode.installForCline` / `installForRooCode` / `installForTrae` / `installForAll` | Per-client install |

### Supported editors

| Editor | Config path (macOS) | Detection |
|---|---|---|
| Cursor | `~/.cursor/mcp.json` | `appName.includes('cursor')` |
| Windsurf | `~/.codeium/windsurf/mcp_config.json` | `appName.includes('windsurf')` |
| Antigravity | `~/.gemini/antigravity/mcp_config.json` | `appName.includes('antigravity')` |
| Trae | `~/Library/Application Support/Trae/mcp.json` | `appName.includes('trae')` |
| VS Code | Claude Desktop config | Default fallback |
| Cline | `Code/User/globalStorage/saoudrizwan.claude-dev/settings/cline_mcp_settings.json` | – |
| Roo Code | `Code/User/globalStorage/rooveterinaryinc.roo-cline/settings/mcp_settings.json` | – |

### Architecture

```
"Sign in to GitHub"
    ↓
vscode.authentication.getSession(GITHUB_SCOPES, createIfNone)
    ↓
OAuth device flow (VS Code handles UI)
    ↓
syncTokenToAllConfigs(session.accessToken)
    ↓
Update all detected MCP configs with GITHUB_TOKEN env
```

### Package guidelines

1. Thin entry point — keep VS Code wiring in `extension.ts`; extract pure helpers for testing
2. Cross-platform (macOS, Linux, Windows)
3. Non-invasive — modify only MCP configs, never editor settings
4. Graceful degradation — handle missing configs, failed auth gracefully
5. Token security — use VS Code's auth API; never log tokens

### Safety (package)

| Path | Access |
|------|--------|
| `src/`, `tests/`, `images/` | ✅ FULL/EDIT |
| `*.json`, `*.config.*` | ⚠️ ASK |
| `out/`, `node_modules/` | ❌ NEVER |

### Manual test checklist

- [ ] Extension activates on startup
- [ ] GitHub OAuth completes
- [ ] Token syncs to all detected MCP configs
- [ ] MCP server starts and responds
- [ ] Works in Cursor, Windsurf, VS Code

---

# Package: `octocode-security-utils`

Standalone security utilities. Self-contained — no special agent guidance beyond access rules and the root core methodology. See `packages/octocode-security-utils/README.md` for API details.
