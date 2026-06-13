# octocode-lsp

Reusable LSP runtime for Octocode.

`octocode-lsp` owns language-server discovery, process startup, stdio JSON-RPC, initialize params, document sync, pooled clients, semantic operations, and local-server configuration. `octocode-mcp` should only consume this package from its MCP tool adapter; it must not carry its own LSP runtime shims.

## Current Quality Bar

This package is the source of truth for Octocode LSP runtime behavior. Keep runtime tests here instead of duplicating language-server lifecycle tests in `octocode-mcp`.

Current gate:

- Unit/integration tests: 55 package tests
- Coverage thresholds: 90% statements, 90% branches, 90% functions, 90% lines
- Real benchmark matrix: TypeScript, JavaScript, Python, Go, Rust, C++, and custom local servers
- Benchmark behavior: unavailable local servers are reported as `SKIP`, not hidden as pass/fail noise

Current verified benchmark result on the maintainer machine:

```text
Summary: 21 passed, 0 failed, 3 skipped
```

The skipped cases were missing local server installs (`pylsp`, `gopls`) and an unavailable `rust-analyzer` rustup component. That is expected: Octocode can discover and run servers, but it does not install every ecosystem server for the user.

## What This Package Owns

- Server registry for built-in languages.
- TypeScript/JavaScript provider selection.
- Custom local server config loading.
- Safe server binary resolution.
- Stdio language-server lifecycle.
- LSP initialize/initialized handshake.
- Document open/change/close tracking.
- Semantic requests:
  - definition
  - references
  - hover
  - document symbols
  - type definition
  - implementation
  - call hierarchy
- Client pooling and cleanup.
- Real benchmark fixtures.

## Package Boundary

The boundary is intentionally simple:

```text
octocode-mcp tool adapter
  -> octocode-lsp public exports
    -> local stdio language servers
```

Do not add LSP runtime files back under `packages/octocode-mcp/src/lsp`. If MCP needs a semantic operation, add or expose it here first, then import it from `octocode-lsp`.

Valid MCP imports look like:

```ts
import { acquirePooledClient } from 'octocode-lsp/manager';
import { resolveWorkspaceRootForFile } from 'octocode-lsp/workspaceRoot';
import type { CodeSnippet } from 'octocode-lsp/types';
```

## TypeScript And JavaScript

The default for `.ts`, `.tsx`, `.mts`, `.cts`, `.js`, `.jsx`, `.mjs`, and `.cjs` is:

```bash
tsgo --lsp --stdio
```

Octocode prefers the packaged or globally available `tsgo` binary. If `tsgo` is not available and the user did not explicitly request it, Octocode falls back to `typescript-language-server --stdio` when that server is installed next to the runtime or available on `PATH`.

Provider override:

```bash
OCTOCODE_TS_LSP_PROVIDER=typescript-language-server
OCTOCODE_TS_LSP_PROVIDER=vtsls
OCTOCODE_TS_LSP_PROVIDER=tsgo
```

Provider table:

| Provider | Command | Package | Role |
| --- | --- | --- | --- |
| `tsgo` | `tsgo --lsp --stdio` | `@typescript/native-preview` | Speed-first default |
| `typescript-language-server` | `typescript-language-server --stdio` | `typescript-language-server` | Stable fallback |
| `vtsls` | `vtsls --stdio` | `@vtsls/language-server` | Alternative TS/JS server |

`tsgo` is Microsoft TypeScript's native preview written in Go. Do not reimplement it in Rust and do not bind it into Node. Spawn it as a normal stdio LSP server.

Resolution order:

1. `OCTOCODE_TS_SERVER_PATH`, using the selected provider's args.
2. Selected provider package binary installed next to the runtime.
3. Selected provider command from `PATH`.
4. `typescript-language-server` fallback when no provider was explicitly requested.

Example custom native-preview binary:

```bash
OCTOCODE_TS_LSP_PROVIDER=tsgo OCTOCODE_TS_SERVER_PATH=/absolute/path/to/tsgo octocode
```

## Other Languages

`tsgo` is not a general multilingual server. Non-TS languages use their own LSP servers from the registry or a custom mapping.

Registry highlights:

| Language | Default command |
| --- | --- |
| Go | `gopls serve` |
| Rust | `rust-analyzer` |
| Python | `pylsp` |
| C/C++ | `clangd` |
| Java | `jdtls` |
| Ruby | `solargraph stdio` |
| PHP | `intelephense --stdio` |
| Swift | `sourcekit-lsp` |
| Dart | `dart language-server --client-id=octocode` |
| Lua | `lua-language-server` |
| Zig | `zls` |
| Elixir | `elixir-ls` |
| Scala | `metals` |
| Haskell | `haskell-language-server-wrapper --lsp` |
| OCaml | `ocamllsp` |
| Clojure | `clojure-lsp` |
| Vue | `vue-language-server --stdio` |
| Svelte | `svelteserver --stdio` |
| YAML | `yaml-language-server --stdio` |
| TOML | `taplo lsp stdio` |
| JSON/JSONC | `vscode-json-language-server --stdio` |
| HTML/CSS/SCSS/LESS | `vscode-html-language-server --stdio` / `vscode-css-language-server --stdio` |
| Shell | `bash-language-server start` |
| SQL | `sql-language-server up --method stdio` |
| GraphQL | `graphql-lsp server -m stream` |
| Terraform | `terraform-ls serve` |

The full registry lives in [packages/octocode-lsp/src/lspRegistry.ts](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-lsp/src/lspRegistry.ts).

Add registry entries only when the language-server command is common, safe to spawn directly, and supports stdio or an equivalent non-shell mode. Otherwise use custom local server config.

`rust-analyzer` gets an extra availability probe with `rust-analyzer --version` because rustup can expose a shim even when the component is missing.

## Server Enablement Map

Octocode does not install every ecosystem server. A built-in server is active when its command is on `PATH`, installed as a package next to the runtime when supported, or provided through that server's env-var override.

| Stack | Command Octocode starts | Override | Current benchmark coverage |
| --- | --- | --- | --- |
| TypeScript/JavaScript | `tsgo --lsp --stdio` by default | `OCTOCODE_TS_LSP_PROVIDER`, `OCTOCODE_TS_SERVER_PATH` | TypeScript and JavaScript |
| Python | `pylsp` | `OCTOCODE_PYTHON_SERVER_PATH` | Python fixture, skipped when unavailable |
| Go | `gopls serve` | `OCTOCODE_GO_SERVER_PATH` | Go fixture, skipped when unavailable |
| Rust | `rust-analyzer` | `OCTOCODE_RUST_SERVER_PATH` | Rust fixture, skipped when unavailable or rustup shim is unhealthy |
| C/C++ | `clangd` | `OCTOCODE_CLANGD_SERVER_PATH` | C++ fixture |
| Custom/private | User config command | `OCTOCODE_LSP_CONFIG` | Custom fixture |

For any other registry language, use the same pattern: install the server command yourself, put it on `PATH` or set the matching `OCTOCODE_*_SERVER_PATH`, then add a benchmark fixture if Octocode should treat it as a supported default.

## Custom Local Servers

Custom local servers are the escape hatch for private languages, internal DSLs, company analyzers, and niche ecosystems.

Config lookup order:

1. `OCTOCODE_LSP_CONFIG`
2. `<workspace>/.octocode/lsp-servers.json`
3. `~/.octocode/lsp-servers.json`

Config shape:

```json
{
  "languageServers": {
    ".foo": {
      "command": "/path/to/foo-lsp",
      "args": ["--stdio"],
      "languageId": "foo",
      "initializationOptions": {
        "analyzerMode": "strict"
      }
    }
  }
}
```

Notes:

- Keys are file extensions including the leading dot.
- `command` must be a direct binary path or executable name.
- Shell commands such as `sh`, `bash`, `zsh`, `cmd.exe`, and `powershell.exe` are rejected.
- `args` are passed as process args, not shell-expanded strings.
- `initializationOptions` are forwarded into the LSP `initialize` request.
- TypeScript-family custom options are shallow-merged over Octocode's TypeScript defaults.

Use this path first when experimenting. A custom mapping can graduate into the built-in registry after it is proven stable across real projects.

Example private DSL:

```json
{
  "languageServers": {
    ".workflow": {
      "command": "/opt/company/bin/workflow-lsp",
      "args": ["--stdio"],
      "languageId": "workflow",
      "initializationOptions": {
        "ruleset": "production"
      }
    }
  }
}
```

Example wrapper-free local server:

```json
{
  "languageServers": {
    ".proto": {
      "command": "buf",
      "args": ["beta", "lsp", "--stdio"],
      "languageId": "proto"
    }
  }
}
```

If a server needs environment setup, login/session bootstrap, or generated config, put that logic in a real executable script and point `command` at the script path. Do not configure `sh -c ...`; shell commands are intentionally rejected.

## Public API

Main entrypoint:

```ts
import {
  LSPClient,
  getLanguageServerForFile,
  isLanguageServerAvailable,
  acquirePooledClient,
  releaseAllPooledClients,
} from 'octocode-lsp';
```

Useful subpath exports:

| Export | Purpose |
| --- | --- |
| `octocode-lsp/client` | Direct `LSPClient` lifecycle |
| `octocode-lsp/manager` | Availability checks and pooled clients |
| `octocode-lsp/config` | Provider and custom-server resolution |
| `octocode-lsp/lspRegistry` | Built-in extension registry |
| `octocode-lsp/workspaceRoot` | Workspace-root inference |
| `octocode-lsp/resolver` | Symbol-to-position resolver |
| `octocode-lsp/types` | Shared semantic types |
| `octocode-lsp/validation` | Safe file reads and server path validation |

Prefer `manager` APIs from MCP/tool code. Direct `LSPClient` usage is mainly for benchmarks and package-level tests.

## Lifecycle Rules

- Acquire pooled clients with `acquirePooledClient(workspaceRoot, filePath)`.
- Do not call `client.stop()` on pooled clients from MCP tool code.
- Use `releasePooledClientForFile` or `releaseAllPooledClients` for explicit cleanup.
- Open documents before semantic requests when using `LSPClient` directly.
- Await `waitForReady()` in benchmarks when measuring semantic operations.
- Always keep file paths absolute at the package boundary.
- Readiness is progress-aware: servers that emit no `$/progress` settle after a short fallback; servers that do emit progress wait for completion or max timeout.
- Shutdown is graceful but bounded. A non-responsive server is terminated instead of blocking pool cleanup indefinitely.

## Adding More Servers

There are three supported paths, ordered from safest to broadest impact.

### 1. Add A Custom Server Mapping

Use this when:

- The language is private or experimental.
- The command is not common enough to assume.
- The server needs company-specific initialization options.
- You want to test support before changing package defaults.

Steps:

1. Create `lsp-servers.json`.
2. Map extension to `{ command, args, languageId }`.
3. Set `OCTOCODE_LSP_CONFIG=/absolute/path/to/lsp-servers.json` or put it in `<workspace>/.octocode/lsp-servers.json`.
4. Run `yarn benchmark custom` if you add a fixture, or call `getLspStatus({ filePath })` from an integration harness.

### 2. Add An Env-Var Override For An Existing Registry Entry

Use this when the extension is already in the registry but the user's server lives somewhere non-standard.

Examples:

```bash
OCTOCODE_GO_SERVER_PATH=/opt/homebrew/bin/gopls
OCTOCODE_RUST_SERVER_PATH=/Users/me/.cargo/bin/rust-analyzer
OCTOCODE_CLANGD_SERVER_PATH=/opt/llvm/bin/clangd
OCTOCODE_PYTHON_SERVER_PATH=/opt/venv/bin/pylsp
```

The env var replaces only the command. The registry args are still used.

### 3. Add A Built-In Registry Entry

1. Add the extension mapping in `src/lspRegistry.ts`.
2. Set the correct LSP `languageId`.
3. Add an env-var override such as `OCTOCODE_GO_SERVER_PATH`.
4. Confirm the server supports stdio or a direct non-shell mode.
5. Add or update a benchmark fixture under `benchmark/<language>`.
6. Run the benchmark with the real server installed.
7. Add an availability health check in `manager.ts` if `command -v` is not enough.
8. Document install expectations in this README only if the server becomes an Octocode-supported default.

Keep the registry boring. A bad default server is worse than no default server; custom local servers cover the long tail.

Built-in entry shape:

```ts
'.ext': {
  command: 'example-lsp',
  args: ['--stdio'],
  languageId: 'example',
  envVar: 'OCTOCODE_EXAMPLE_SERVER_PATH',
}
```

Acceptance checklist:

- The command is safe to spawn without a shell.
- The server reads/writes LSP over stdio.
- The `languageId` matches the server's expected document language.
- The server starts from the workspace root.
- The server works with absolute file URIs.
- At least definition, references, hover, and document symbols are benchmarked when the server supports them.
- Unsupported operations are either capability-gated or omitted from the benchmark expectation.

## Benchmarks

Build first:

```bash
yarn build
```

Run TypeScript and custom-server proof:

```bash
yarn benchmark typescript custom
```

Run the full matrix:

```bash
yarn benchmark
```

Compare TS providers:

```bash
OCTOCODE_TS_LSP_PROVIDER=tsgo yarn benchmark typescript
OCTOCODE_TS_LSP_PROVIDER=typescript-language-server yarn benchmark typescript
```

The benchmark reports:

- server command and args
- startup time
- readiness wait
- operation timings
- pass/fail/skip counts

Current matrix:

| Case | Server | Expected when server exists |
| --- | --- | --- |
| `typescript` | `tsgo` or selected TS provider | definition, references, hover, document symbols, type definition, implementation, call hierarchy |
| `javascript` | `tsgo` or selected TS provider | definition, references, hover, document symbols, call hierarchy |
| `python` | `pylsp` | definition, references, hover, document symbols, call hierarchy |
| `go` | `gopls serve` | definition, references, hover, document symbols, type definition, implementation, call hierarchy |
| `rust` | `rust-analyzer` | definition, references, hover, document symbols, type definition, implementation, call hierarchy |
| `cpp` | `clangd` | definition, references, hover, document symbols, implementation |
| `custom` | fixture server from `benchmark/custom/lsp-servers.json` | definition, references, hover, document symbols |

Benchmark skips are acceptable only when the server is not installed or fails its availability health check. Benchmark failures mean either the package regressed or the fixture expectation does not match that server's real capability profile.

The `custom` fixture starts a local `.foo` LSP server from `benchmark/custom/lsp-servers.json` through `OCTOCODE_LSP_CONFIG` and verifies:

- custom extension mapping
- custom command startup
- custom `languageId`
- `initializationOptions` pass-through
- definition
- references
- hover
- document symbols

## Verification

Recommended package checks:

```bash
yarn typecheck
yarn lint
yarn test
yarn build
yarn benchmark
```

When validating MCP integration, run MCP checks from `packages/octocode-mcp`, but keep runtime assertions in this package. MCP tests should validate MCP schemas, tool envelopes, routing, and calls into `octocode-lsp`, not duplicate LSP runtime behavior.

## Opportunities

Highest-value next upgrades:

- Expand real fixtures for registry languages that are already mapped but not yet benchmarked: Java, PHP, Ruby, Swift, Lua, YAML, JSON, HTML/CSS, GraphQL, Terraform, and shell.
- Add server-specific health checks for wrappers and shims where `which <command>` is not enough, following the `rust-analyzer --version` pattern.
- Add capability profiles so benchmarks can report `unsupported by server` instead of hand-tuning expected operations per fixture.
- Add startup diagnostics that distinguish command missing, command present but unhealthy, initialize timeout, protocol mismatch, and missing project metadata.
- Add optional JSON benchmark output for CI dashboards and release comparison.
- Add a compatibility table tracking tested server versions, OSes, benchmark status, and known limitations.
- Add packaged-server resolution for npm-distributed servers such as YAML, JSON, HTML, CSS, Vue, Svelte, GraphQL, and Bash where package metadata exposes a safe bin entry.
- Add language-specific initialization option examples only after real users need them; avoid speculative config docs.

Good product opportunities:

- Private DSL support through checked-in `.octocode/lsp-servers.json` examples.
- Polyglot monorepo diagnostics that show exactly which language servers are active, missing, skipped, or unhealthy.
- Provider comparisons for TS/JS (`tsgo` vs `typescript-language-server` vs `vtsls`) using the same benchmark fixture and timing output.
- A small compatibility command that prints the detected server command, args, version/health check result, and benchmark hint for a file.

## Troubleshooting

`No language server is available`

- Confirm the file extension is in `src/lspRegistry.ts` or a custom config.
- Run `command -v tsgo`, `command -v gopls`, or the relevant server command.
- For TypeScript/JavaScript, install `@typescript/native-preview` or set `OCTOCODE_TS_LSP_PROVIDER=typescript-language-server`.
- For Rust, `command -v rust-analyzer` is not enough if rustup exposes a missing-component shim. Run `rust-analyzer --version`.

`Server exits before initialize`

- Run the configured command manually with the same args.
- Check whether the server expects `--stdio`, `stdio`, `serve`, or another mode.
- Use `OCTOCODE_LSP_CONFIG` to test without changing global config.
- If `isLanguageServerAvailable()` says true but startup still fails, add a targeted health check in `manager.ts`.

`Custom server config ignored`

- Check that the extension key includes the leading dot.
- Check JSON against the `languageServers` shape above.
- Avoid shell wrappers; point directly at the binary.
- Remember lookup order: env config path beats workspace config, which beats global config.
