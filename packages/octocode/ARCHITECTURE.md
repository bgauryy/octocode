# Octocode CLI Architecture

`octocode` is the public **CLI and installer** package. It is a thin
presentation layer: it parses input, routes to a handler, and renders output.
Tool execution, pagination, and security come from
`@octocodeai/octocode-tools-core`, which calls the native engine. Public schemas,
descriptions, shared instructions, and reusable output types come from
`@octocodeai/octocode-core`. Nothing in this package shapes tool data; it only
formats it for a terminal.

## Boundary

- `src/index.ts` is the binary entry (`bin: out/octocode.js`). It calls
  `runCLI()`, falls back to top-level help, and delegates signal termination to
  `src/cli/process-lifecycle.ts` so structured stdout stays clean.
- `src/cli/index.ts` (`runCLI`) is the dispatcher: parse args → handle global
  flags (`--help`, `--version`, `--no-color`) → route to a command,
  the `tools`/`context` surface, or interactive install. It also invokes
  `stale-build.ts`, which compares the running bundle with every TypeScript
  input under `src/` when dogfooding `out/`.
- Keep dispatch thin. New behavior belongs in a command or feature module, not
  in `runCLI`.

## Layers

- `src/cli/` — argument parsing (`parser.ts`), routing (`routing.ts`, the
  local-vs-GitHub ref resolver), validation, help rendering, and exit codes
  (`exit-codes.ts`).
- `src/cli/commands/` — one file per command. Two groups:
  - **Quick commands** (`cache`) — a thin shortcut that resolves a target ref
    and materializes it locally.
    Legacy research shortcuts (`cat`, `ls`, `find`, `grep`, `history`, `repo`,
    `pkg`, `lsp`, `binary`, `unzip`, `diff`, `pr`) and the unified `search`
    command are intentionally removed; use `tools <name>` for research.
  - **Management commands** (`install`, `auth`/`login`/`logout`, `status`) —
    lazy-loaded from the command registry; manage setup, credentials, and
    environment state. `skill` and `lsp-server` own their respective local
    management flows.
- `src/cli/tool-command/` — the raw `tools <name>` / `context` surface. Bridges
  to `octocode-tools-core/direct` for execution and `octocode-core/schema` for
  schemas, display fields, examples, and input preparation. `octocode-core/mcp`
  owns CLI context and shared MCP guidance; the CLI supplies runtime availability.
- `src/ui/` — interactive TUI: the menu loop (`menu/main-menu.ts`), install flow
  (`install/`), and MCP configuration editor (`config/`). Reached via
  `octocode install` → `runInteractiveMode`.
- `src/features/` — stateful operations behind commands/UI: MCP `install`,
  GitHub `github-oauth` / `gh-auth`, registry `sync`, and `node-check`.
- `src/cli/commands/skills/` — bundled-skill registry, checks, removal,
  environment readiness, and CLI rendering. Cross-platform destination mapping,
  durable canonical materialization, conflict handling, and linking come from
  `@octocodeai/octocode-skill-installer`.
- `src/utils/` — terminal primitives (colors, spinner, prompts), MCP config I/O,
  token storage, platform/shell/fs helpers, and frontmatter parsing.
  MCP client discovery and paths come from `mcp-paths.ts`; config reads and
  writes come from `mcp-io.ts`. `mcp-config.ts` owns configuration composition
  and installation status, using the shared client inventory.

## Build

- `build.mjs` bundles `src/index.ts` with esbuild → `out/octocode.js`
  (ESM, minified, code-split, with a `#!/usr/bin/env node` shebang).
- Published runtime dependencies, including
  `@octocodeai/octocode-tools-core`, `@octocodeai/octocode-engine`, and
  `@octocodeai/octocode-skill-installer`, stay
  external so each package owns and resolves its own dependency graph. The
  native `.node` binary comes from the engine package's platform
  `optionalDependencies`.
- The build inspects esbuild's metafile and fails if output contains a bare
  external import that the CLI does not declare as a runtime dependency.
- `__APP_VERSION__` is injected at build time from `package.json`.

## Publish Boundary

Publish runtime prerequisites before the CLI: engine platform packages, the
engine root, config/core/tools-core, the skill installer, and then `octocode`.
The CLI declares only the packages it imports directly, including core contracts
and tools-core execution.

## Rules

- The CLI renders; it does not compute. Push any data-shaping into
  `octocode-tools-core`.
- Lazy-load command and tool modules (dynamic `import`) to keep startup fast and
  tolerate a missing tool runtime gracefully.
- Run `yarn verify` (lint + typecheck + test + package validations) before
  publishing, then exercise the built `out/octocode.js` path.
