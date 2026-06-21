---
name: octocode
description: Use for Octocode monorepo work: architecture, package boundaries, local development, building octocode-core, wiring workspace deps, and dogfooding the built CLI.
---

# Octocode Monorepo Guide

Use this skill when changing or investigating `/Users/guybary/Documents/octocode-mcp`.

## Architecture

Octocode is a code-research toolset exposed through thin interfaces:

```
octocode-mcp          octocode CLI          octocode-vscode
    |                     |                      |
    +---------- depend on @octocodeai/octocode-tools-core
                                  |
                                  +-- @octocodeai/octocode-core
                                  |   external repo: schemas, tool descriptions, prompt
                                  |
                                  +-- @octocodeai/octocode-engine
                                      Rust/napi primitives
```

Package roles:

- `packages/octocode-tools-core`: the brain. Tool runners, GitHub/local/LSP providers, credentials, config, hints, security wrappers.
- `packages/octocode-engine`: Rust/napi primitives. Minify, signatures, structural search, ripgrep parsing, LSP, secret scan/sanitize, text utilities.
- `packages/octocode-mcp`: thin MCP stdio server. Register tools, lifecycle, output sanitization.
- `packages/octocode`: thin CLI. Parse commands, render output, run the same tool runners as MCP.
- `packages/octocode-vscode`: VS Code extension for auth, MCP install, token sync.
- `@octocodeai/octocode-core`: external package in `/Users/guybary/Documents/octocode-mcp-host/packages/octocode-core`. It owns tool schemas, descriptions, CLI command specs, and the system prompt.

Golden rule: business logic belongs in `octocode-tools-core`, schemas and user-facing tool guidance belong in `octocode-core`, native primitives belong in `octocode-engine`, and interface packages stay thin.

## Local Dev Flow

Always build the code path you are about to test, then dogfood the built CLI from this repo.

1. If you changed `octocode-core`, build it first:

```bash
cd /Users/guybary/Documents/octocode-mcp-host/packages/octocode-core
yarn build
```

2. In the Octocode repo, make Yarn refresh deps from the workspace/local package graph:

```bash
cd /Users/guybary/Documents/octocode-mcp
yarn install
```

Internal packages should resolve through `workspace:*`. `@octocodeai/octocode-core` is the exception because it lives in the sibling `octocode-mcp-host` repo; local development may use a `file:` dependency to that package. Do not leave a machine-specific absolute `file:` path in a commit unless the user explicitly asks for local-only changes.

3. Build everything from the monorepo root:

```bash
yarn build
```

This is the trusted path. It rebuilds local packages so CLI/MCP output reflects local source, not stale `out/` or published packages.

4. Verify through the built CLI:

```bash
node /Users/guybary/Documents/octocode-mcp/packages/octocode/out/octocode.js --help
node /Users/guybary/Documents/octocode-mcp/packages/octocode/out/octocode.js context
node /Users/guybary/Documents/octocode-mcp/packages/octocode/out/octocode.js tools <tool-name> --scheme
node /Users/guybary/Documents/octocode-mcp/packages/octocode/out/octocode.js tools <tool-name> --queries '<json>'
```

Use a system Node when native addons fail under the Codex app runtime:

```bash
/opt/homebrew/bin/node /Users/guybary/Documents/octocode-mcp/packages/octocode/out/octocode.js <command>
```

If CLI output does not show your local `octocode-core` or package change, rebuild `octocode-core`, run `yarn install`, run root `yarn build`, and retry the built CLI.

## Dogfood Rules

Use the built CLI for repo research whenever possible:

```bash
node packages/octocode/out/octocode.js ls   ./packages/octocode/src
node packages/octocode/out/octocode.js cat  ./packages/octocode/src/cli/index.ts --mode symbols
node packages/octocode/out/octocode.js grep toolName ./packages --compact
node packages/octocode/out/octocode.js find ./packages --name '*.test.ts'
node packages/octocode/out/octocode.js lsp  ./packages/octocode/src/cli/index.ts --symbol runCLI --type references
```

Raw tools are available through `tools <name>`. Read the schema first:

```bash
node packages/octocode/out/octocode.js tools
node packages/octocode/out/octocode.js tools localSearchCode --scheme
node packages/octocode/out/octocode.js tools localSearchCode \
  --queries '{"path":"./packages","keywords":"runCLI","mode":"discovery"}' --compact
```

Prefer the CLI over raw `cat`/`grep`/`find` for reading, searching, structure, LSP, history, package lookup, clone, and binary inspection. Fall back to shell only when the CLI is broken, unavailable, or the task is simple command output.

After each session, append one concise dated line to `.octocode/CLI_OVERVIEW.md`:

- `## Good flows`: command or chain that worked well.
- `## Known limits`: friction, bug, or doc gap discovered while dogfooding.

## Tool Surface

The same 13 tools run through MCP and CLI:

- GitHub: `ghSearchCode`, `ghGetFileContent`, `ghViewRepoStructure`, `ghSearchRepos`, `ghHistoryResearch`, `ghCloneRepo`
- Package: `npmSearch`
- Local: `localSearchCode`, `localViewStructure`, `localFindFiles`, `localGetFileContent`, `localBinaryInspect`
- LSP: `lspGetSemantics`

All tools accept bulk `queries`. The CLI fills common research metadata for local/raw calls, but GitHub/package work may still need `mainResearchGoal`.

Useful read/search defaults:

- Start cheap: `ls`, `grep --compact`, `localSearchCode mode:"discovery"`, or `cat --mode symbols`.
- Read exact text only when needed: use `--mode none` or tool `minify:"none"` for quotes/diffs.
- For raw `tools` calls, never guess fields; run `tools <name> --scheme`.
- For LSP calls, get `uri`, `symbolName`, and `lineHint` from a real search/read result. Do not invent line numbers.

## Change Placement

- Tool descriptions, schema text, command specs, system prompt: `octocode-core`.
- Tool behavior, providers, GitHub/local/LSP orchestration: `octocode-tools-core`.
- Native search/minify/LSP/secret/text primitives: `octocode-engine`.
- CLI parsing/rendering only: `packages/octocode`.
- MCP lifecycle/registration only: `packages/octocode-mcp`.
- VS Code auth/install/sync only: `packages/octocode-vscode`.

No backward compatibility burden by default. Rename, delete, and simplify when that is the clean fix, unless the user explicitly asks for compatibility.

## Verification

Use the narrowest meaningful check, then finish with the real path:

```bash
yarn test
yarn lint
yarn typecheck
yarn build
node packages/octocode/out/octocode.js <command-that-proves-the-change>
```

For Rust/native work, use the `rust-package-node` skill and include `cargo test` or the relevant native build command.
