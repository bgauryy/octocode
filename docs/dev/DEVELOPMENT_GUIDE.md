# Development Guide

> Development standards, workflows, and reference material for the Octocode monorepo.

## Safety & Permissions

### Approval Policy

| Action | Approval | Notes |
|--------|----------|-------|
| Edit `src/`, `tests/` | ✅ Auto | Standard development |
| Edit `docs/` | ✅ Auto | Documentation updates |
| Edit configs | ⚠️ Ask | `tsconfig`, `vitest`, `eslint`, `rollup` |
| Add dependencies | ⚠️ Ask | Requires `yarn add` |
| Edit Secrets | ❌ Never | `.env` files, keys |
| Edit Generated | ❌ Never | `dist/`, `out/`, `coverage/` |

### Protected Files

- **Never Modify**: `.env*`, `yarn.lock` (modify via yarn), `.git/`, `dist/`, `out/`, `coverage/`
- **Ask Before Modifying**: `package.json`, `tsconfig*.json`, `vitest.config.ts`, `rollup.config.js`, `.eslintrc.json`

## Commands & Workflow

**Use `yarn` for all package management.**

| Task | Command | Scope |
|------|---------|-------|
| **Install** | `yarn install` | All packages |
| **Build** | `yarn build` | All packages |
| **Test** | `yarn test` | All packages (coverage report) |
| **Test (Quiet)**| `yarn test:quiet` | Minimal output |
| **Lint** | `yarn lint` | All packages |
| **Lint Fix** | `yarn lint:fix` | All packages |
| **Syncpack** | `yarn syncpack:lint` | Check dependency versions |

### Package-Specific Commands

| Package | Key Commands |
|---------|--------------|
| `octocode-mcp` | `yarn mcp:package`, `yarn mcp:contracts`, `yarn debug`, `yarn build:watch` |
| `octocode-cli` | `yarn start`, `yarn validate:mcp`, `yarn validate:skills` |
| `octocode-vscode` | `yarn package`, `yarn publish` |
| `octocode-shared` | `yarn shared:package`, `yarn typecheck` |

#### Linux & File Operations

- **String Replacement**: `sed -i '' 's/old/new/g' src/**/*.ts`
- **Move/Copy**: `mv`, `cp`, `rsync` for file operations
- **Find & Move**: `find src -name "*.test.ts" -exec mv {} tests/ \;`
- **Content Extract**: `head`, `tail`, `cat`, `grep`
- **Bulk Actions**: Prefer Linux one-liners for simple operations
- **Complex Tasks**: Write scripts (Node.js, Python, Shell)

## Development Standards

### Style Guide

- **Language**: TypeScript (strict mode)
- **Formatting**: Semicolons: Yes, Quotes: Single, Width: 80, Tab: 2
- **Code Style**: Prefer `const`. Explicit return types. No `any`. Use `?.` and `??`.

### Naming Conventions

| Type | Convention | Example |
|------|------------|---------|
| Functions | `camelCase` | `fetchData()` |
| Classes | `PascalCase` | `TokenManager` |
| Constants | `UPPER_SNAKE_CASE` | `MAX_RETRIES` |
| Files | `camelCase.ts` or `kebab-case.ts` | `toolConfig.ts` |
| Tests | `<name>.test.ts` | `session.test.ts` |

### Dependencies

- **Node.js**: >= 20.0.0
- **VS Code**: `octocode-vscode` requires >= 1.85.0
- **Core**: `@modelcontextprotocol/sdk` (^1.29.0), `zod`, `vitest`, `typescript`
- **LSP**: `typescript-language-server`, `vscode-languageserver-protocol`

## Testing Protocol

### Requirements
- **Coverage**: 90% required for `octocode-mcp` (Statements, Branches, Functions, Lines)
- **Framework**: Vitest with V8 coverage provider

### Quality Lanes

| Package | Lane | Command | This lane should fail when... |
|---------|------|---------|-------------------------------|
| `octocode-mcp` | Package gate | `yarn mcp:package` | Shipping runtime code, startup/config contracts, provider execution, response envelopes, or declared user flows regress. |
| `octocode-mcp` | Contract suite | `yarn mcp:contracts` | You need the fast deterministic contract signal while iterating locally. |
| `octocode-shared` | Package gate | `yarn shared:package` | Shared config, credential, platform, or session boundaries regress. |

For package-specific references, see the consolidated [docs index](https://github.com/bgauryy/octocode-mcp/blob/main/docs/README.md) — `octocode-shared` API/credentials/session docs and `octocode-cli` reference all live there.

### Structure
```
packages/<name>/tests/
├── <module>.test.ts       # Unit tests
├── integration/           # Integration tests
├── security/              # Security-focused tests
├── github/                # GitHub API tests
├── lsp/                   # LSP tool tests
└── helpers/               # Test utilities
```

## Research Workflows

For detailed research workflows including LSP navigation, local discovery, and external research patterns, see the canonical references:

- [Local & LSP Tools Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/LOCAL_TOOLS_REFERENCE.md) — Local discovery, LSP navigation, and flow tracing
- [GitHub, GitLab & Bitbucket Tools Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/GITHUB_GITLAB_TOOLS_REFERENCE.md) — External research and package discovery
- [Clone & Local Tools Workflow](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/workflows/CLONE_AND_LOCAL_TOOLS_WORKFLOW.md) — Bridging GitHub repos with local + LSP tools

## Skills System

Skills are markdown-based instruction sets that teach AI assistants specific tasks. For the complete skills guide including installation, creating custom skills, and the marketplace:

- [Skills Guide](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/SKILLS_GUIDE.md) — Comprehensive guide to the skills system
- [Skills Index](https://github.com/bgauryy/octocode-mcp/blob/main/skills/README.md) — All available skills with when-to-use guide

## Package Documentation

For the complete package documentation index, see [docs/README.md](https://github.com/bgauryy/octocode-mcp/blob/main/docs/README.md).


## Agent Compatibility

| Agent | Setup |
|-------|-------|
| **Cursor** | Reads `AGENTS.md` automatically |
| **Claude Code** | Reads `AGENTS.md` as context |
| **Aider** | Add `read: AGENTS.md` in `.aider.conf.yml` |
| **Gemini CLI** | Set `"contextFileName": "AGENTS.md"` in `.gemini/settings.json` |

## See Also

- [Configuration Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/CONFIGURATION_REFERENCE.md) — All env vars and `.octocoderc` options
- [Troubleshooting](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/TROUBLESHOOTING.md) — Common issues and solutions
- [Authentication Setup](https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/providers/AUTHENTICATION_SETUP.md) — GitHub/GitLab/Bitbucket auth
