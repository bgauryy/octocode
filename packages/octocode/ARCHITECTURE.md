# Octocode CLI Architecture

`octocode` is the user-facing **CLI and installer** package. It is a thin
presentation layer: it parses input, routes to a handler, and renders output.
At source/build time, all tool logic — schemas, execution, pagination,
security — comes from `@octocodeai/octocode-tools-core` (which in turn calls the
native `@octocodeai/octocode-engine` and reads metadata from
`@octocodeai/octocode-core`). Nothing in this package shapes tool data; it only
formats it for a terminal.

## Boundary

- `src/index.ts` is the binary entry (`bin: out/octocode.js`). It calls
  `runCLI()`, falls back to top-level help, and owns process signals/exit.
- `src/cli/index.ts` (`runCLI`) is the dispatcher: parse args → handle global
  flags (`--help`, `--version`, `--context`, `--no-color`) → route to a command,
  the `tools`/`context` surface, or interactive install.
- Keep dispatch thin. New behavior belongs in a command or feature module, not
  in `runCLI`.

## Layers

- `src/cli/` — argument parsing (`parser.ts`), routing (`routing.ts`, the
  local-vs-GitHub ref resolver), validation, help rendering, and exit codes
  (`exit-codes.ts`).
- `src/cli/commands/` — one file per command. Two groups:
  - **Quick commands** (`search`, `clone`, `cache`) — thin
    shortcuts that resolve a target ref and call the underlying tool/OQL route.
    Legacy research shortcuts (`cat`, `ls`, `find`, `grep`, `history`, `repo`,
    `pkg`, `lsp`, `binary`, `unzip`, `diff`, `pr`) are intentionally removed; use
    `search`.
  - **Management commands** (`install`, `auth`/`login`/`logout`, `status`) —
    eagerly loaded; manage setup, credentials, and environment state.
- `src/cli/tool-command.ts` — the raw `tools <name>` / `context` surface. Bridges
  directly to `octocode-tools-core/direct` (`executeDirectTool`, schema text,
  display fields) for power users and agents.
- `src/ui/` — interactive TUI: the menu loop (`menu.ts`), install flow
  (`install/`), config inspector (`config/`), and skills marketplace
  (`skills-menu/`). Reached via `octocode install` → `runInteractiveMode`.
- `src/features/` — stateful operations behind commands/UI: MCP `install`,
  GitHub `github-oauth` / `gh-auth`, `skills` install, registry `sync`, and
  `node-check`.
- `src/configs/` — static, Zod-validated catalogs: `mcp-registry.ts` (installable
  MCP servers) and `skills-marketplace.ts` (skill sources). Validated by
  `scripts/validate-*.ts`.
- `src/utils/` — terminal primitives (colors, spinner, prompts), MCP config I/O,
  token storage, platform/shell/fs helpers, and frontmatter parsing.

## Build

- `build.mjs` bundles `src/index.ts` with esbuild → `out/octocode.js`
  (ESM, minified, code-split, with a CJS-compat banner and a `#!/usr/bin/env node`
  shebang).
- `@octocodeai/octocode-tools-core` is deliberately bundled into the CLI output.
  It is a workspace `devDependency`, not a package npm users install.
- All published runtime `dependencies`, especially
  `@octocodeai/octocode-engine`, `@octocodeai/octocode-core`, and `zod`, stay
  **external** so npm resolves them normally. The native `.node` binary comes
  from the engine package's platform `optionalDependencies`.
- `__APP_VERSION__` is injected at build time from `package.json`.

## Publish Boundary

`octocode` is published after `@octocodeai/octocode-engine` because the CLI
declares the engine as a direct runtime dependency. `@octocodeai/octocode-tools-core`
is absent from the publish order: its code is already inside `out/octocode.js`.

## Rules

- The CLI renders; it does not compute. Push any data-shaping into
  `octocode-tools-core`.
- Lazy-load command and tool modules (dynamic `import`) to keep startup fast and
  tolerate a missing tool runtime gracefully.
- Keep `mcp-registry` / `skills-marketplace` schema-valid; run `yarn verify`
  (lint + typecheck + test + registry/skills validation) before publishing.
