# Octocode Monorepo — Package Overview

12 packages, one purpose: give AI agents fast, precise, evidence-backed access
to code — locally and on GitHub.

---

## Architecture in one picture

```
┌─────────────────────────────────────────────────────────┐
│  INTERFACES (what users install / run)                   │
│                                                          │
│  octocode (CLI)   octocode-mcp   octocode-vscode        │
│  octocode-native  octocode-pi-extension                  │
└──────────────────────┬──────────────────────────────────┘
                       │ all depend on
┌──────────────────────▼──────────────────────────────────┐
│  BRAIN                                                   │
│  octocode-tools-core  ← all tool runners live here      │
│       ├── contracts ── octocode-core  (external sibling) │
│       ├── native ───── octocode-engine  (Rust/napi)     │
│       └── config ───── octocode-config                  │
└─────────────────────────────────────────────────────────┘

  Extras:  octocode-skill-installer  octocode-awareness
           octocode-extension-rust   octocode-benchmark
```

---

## Packages

### `octocode` — npm: `octocode`
**The Node.js CLI users install today.**

`npx octocode` / `npm install -g octocode` / `brew install bgauryy/octocode/octocode`

Thin shell over `octocode-tools-core`. Handles human CLI commands (`search`,
`files`, `symbols`, `ast`, `graph`, `def`, `refs`, `repos`, `code`, `history`,
`clone`, `package`, `context`, `auth`, `install`, `skill`), interactive IDE
management, auth flows, and skill install. Bundled with esbuild into a single
CJS file. Also builds as a Node SEA (Single Executable Application) for
distribution without Node.

---

### `octocode-mcp` — npm: `octocode-mcp`
**The MCP stdio server that AI assistants connect to.**

`npx octocode-mcp` / used by Cursor, Windsurf, Claude Desktop, VS Code + Copilot

Registers the full 11-tool catalog (localSearch, localFetch, astSearch,
lspSearch, ghSearch, ghGetFileContent, ghSearchHistory, ghGetHistoryItem,
ghCloneRepo, artifactSearch, astRewrite) as MCP tools. Zero business logic —
lifecycle, security, registration, sanitized output only. All execution
delegates to `octocode-tools-core`.

---

### `octocode-native` — npm: `@octocodeai/octocode-native`
**The Rust CLI. Same tools, no Node dependency.**

`npx @octocodeai/octocode-native` / `npm install -g @octocodeai/octocode-native`

A pure Rust binary (`octocode`) that runs the full tool catalog without Node.
Distributed via npm `optionalDependencies` (esbuild/Biome pattern) — 6
platform packages, auto-selected at install time. Also builds as a NAPI `.node`
addon so the MCP/Node path can call into the Rust runtime directly.

**Crate name:** `octocode-native` (crates.io). **Binary name:** `octocode`.

---

### `octocode-tools-core` — npm: `@octocodeai/octocode-tools-core`
**The brain. All 11 tool runners live here.**

Never installed by users directly — depended on by `octocode`, `octocode-mcp`,
`octocode-pi-extension`. Owns GitHub/Octokit client, security policy, provider
registry, credential management, session, config resolution, and response
shaping. Delegates home/env to `@octocodeai/config`, native primitives to
`octocode-engine`. Tool registry: `src/tools/toolConfig.ts`.

---

### `octocode-engine` — npm: `@octocodeai/octocode-engine`
**Rust/napi primitives consumed by tools-core.**

Not a user-facing package. Provides the fast Rust implementations of: ripgrep
search, AST structural search (tree-sitter), context minification, secret
detection, and an LSP client pool (definition, references, hover, callers,
types, diagnostics). Built as a `.node` napi addon, consumed by tools-core via
`require()`. Platform packages: `@octocodeai/octocode-engine-darwin-arm64`
etc.

---

### `octocode-config` — npm: `@octocodeai/config`
**Zero-dependency env + config loader. Single source of truth.**

`import { getOctocodeHome, parseEnv, loadOctocoderc, propagateOctocodeEnv } from '@octocodeai/config'`

Used by every other package. Owns `OCTOCODE_HOME` resolution, `.env` parsing,
`.octocoderc` loading, and `PROTECTED_KEYS`. Also ships a small CLI:
`npx @octocodeai/config --keys`. Skills get it injected as `octocode-config.mjs`.
**Never reimplement these in another package.**

---

### `octocode-skill-installer` — npm: `@octocodeai/octocode-skill-installer` (private)
**Shared skill materialization logic bundled into CLI/MCP builds.**

Not published standalone. Owns durable skill install, platform paths
(symlinks on Unix, junctions on Windows), conflict policy, and install result
reporting. Both `octocode` CLI and the Awareness skill commands share this so
skill install behavior is identical everywhere.

---

### `octocode-awareness` — npm: `@octocodeai/octocode-awareness`
**Agent coordination runtime: plans, memory, locks, messages, reflection.**

`npm install @octocodeai/octocode-awareness`

Gives coding agents shared state: work plans, progress tracking, peer
messages, memory/recall, verification debt, and physiological hooks (resource
awareness). Uses SQLite + embeddings locally. Exposes a Pi-facing subset via
`@octocodeai/octocode-awareness/host`. Used by the `octocode-awareness` skill.

---

### `octocode-pi-extension` — npm: `@octocodeai/pi-extension`
**Official Pi coding-agent integration package.**

Wires Octocode CLI and MCP into Pi, registers native tools, bundles Awareness
assets, owns Pi-facing prompt contracts, and handles harness hooks. Pi-owned
protocol/capability/discovery/path contracts live under
`src/contracts/`. After changing the Awareness host API, build Awareness before
rebuilding Pi extension.

---

### `octocode-vscode` — npm: `octocode-mcp-vscode`
**VS Code / multi-editor management extension.**

Distributed as a `.vsix` via the VS Code Marketplace. Handles GitHub OAuth,
MCP server installation into Cursor, Windsurf, Claude Desktop, Copilot, and
others, and token sync across editors. No tool execution — purely management UI.

---

### `octocode-extension-rust` — npm: `@octocodeai/octocode-extension-rust`
**Native filesystem integrity primitives for the Pi extension.**

Separate from `octocode-engine`. Owns filesystem snapshots, atomic mutations,
durability, and line-level diff for the extension layer. Built as a napi addon
like engine but with a narrower scope — only the extension uses it.

---

### `octocode-benchmark` — npm: `@octocodeai/octocode-benchmark` (private)
**Internal benchmarks and evals. Never published.**

Head-to-head comparisons: octocode vs `gh`, `gh+rtk`, `ast-grep`. Uses VRPT
(Verified Result Passage Test) scoring. Run via the `octocode-eval-benchmark`
skill. Lives in `packages/octocode-benchmark/`. Raises the bar, does not ship.

---

## Publish status

| Package | npm name | Published | Notes |
|---|---|---|---|
| `octocode` | `octocode` | ✅ v19.2.0 | Main Node CLI |
| `octocode-mcp` | `octocode-mcp` | ✅ v19.2.0 | MCP server |
| `octocode-native` | `@octocodeai/octocode-native` | 🔜 v0.1.0 | Native CLI — see [PUBLISHING.md](../packages/octocode-native/docs/PUBLISHING.md) |
| `octocode-tools-core` | `@octocodeai/octocode-tools-core` | ✅ v19.2.0 | Brain |
| `octocode-engine` | `@octocodeai/octocode-engine` | ✅ v19.2.0 | Rust/napi engine |
| `octocode-config` | `@octocodeai/config` | ✅ v20.0.0 | Config loader |
| `octocode-awareness` | `@octocodeai/octocode-awareness` | ✅ v2.1.0 | Agent coordination |
| `octocode-pi-extension` | `@octocodeai/pi-extension` | ✅ v19.2.0 | Pi integration |
| `octocode-vscode` | `octocode-mcp-vscode` | ✅ v19.2.0 | VS Code extension |
| `octocode-extension-rust` | `@octocodeai/octocode-extension-rust` | ✅ v0.1.0 | Extension native |
| `octocode-skill-installer` | `@octocodeai/octocode-skill-installer` | ❌ private | Bundled into callers |
| `octocode-benchmark` | `@octocodeai/octocode-benchmark` | ❌ private | Internal evals only |

---

## Key rules

- **Tool execution** lives only in `octocode-tools-core`. Never duplicate in interface packages.
- **Config/env** flows only through `@octocodeai/config`. Never reimplement `getOctocodeHome`.
- **Contracts** (tool names, schemas, descriptions) live in the external sibling `@octocodeai/octocode-core`. Import from there, don't hand-write.
- **Native primitives** live in `octocode-engine` (consumed by tools-core) or `octocode-extension-rust` (consumed by Pi extension only). Never duplicate Rust logic.
- **Skill install** logic lives in `octocode-skill-installer` — both CLI and Awareness bundle it.

## Related

- [OCTOCODE_TOOLS.md](OCTOCODE_TOOLS.md) — full tool catalog
- [OCTOCODE_MCP.md](OCTOCODE_MCP.md) — MCP server setup
- [CONFIGURATION.md](CONFIGURATION.md) — env and config keys
- [packages/octocode-native/docs/PUBLISHING.md](../packages/octocode-native/docs/PUBLISHING.md) — native CLI publish guide
- [AGENTS.md](../AGENTS.md) — agent instructions for this repo
