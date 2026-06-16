# octocode-lsp

Rust-native LSP runtime for Octocode.

`octocode-lsp` owns language detection, language-server command resolution, stdio JSON-RPC, initialize handling, document sync, pooled clients, symbol anchoring, and semantic requests. Tool packages should import this package instead of carrying their own LSP runtime.

## Quick Start

From `packages/octocode-lsp`:

```bash
yarn build:dev
yarn benchmark typescript javascript rust
```

Run the full live matrix:

```bash
yarn benchmark typescript javascript python go rust cpp custom
yarn benchmark --iterations=5 --json typescript rust
```

Current verified live result on the maintainer machine:

```text
Summary: 39 passed, 0 failed, 1 skipped
```

The one skip is Python call hierarchy because `pylsp` does not advertise `callHierarchyProvider`.

## What Ships Here

- Native Node addon built with napi-rs.
- Rust-backed language and server registry.
- Rust-backed Tree-sitter symbol anchoring.
- Stdio JSON-RPC client for local language servers.
- Pooled LSP clients for tool runtime use.
- Capability-gated semantic operations.
- Optional per-platform native packages such as `octocode-lsp-darwin-arm64`.

Core source:

- [Rust config registry](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-lsp/src/config.rs)
- [Rust grammar registry](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-lsp/src/grammar.rs)
- [Native client](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-lsp/src/client.rs)
- [JS wrapper](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-lsp/src/client.ts)

## Supported Languages

Supported means the package has Rust-backed extension detection and symbol anchoring. LSP operations also need the listed server installed or configured.

| Language | Extensions | Native anchors | Default LSP command | Override |
| --- | --- | --- | --- | --- |
| TypeScript | `.ts`, `.mts`, `.cts` | Yes | `typescript-language-server --stdio` | `OCTOCODE_TS_SERVER_PATH` |
| TSX | `.tsx` | Yes | `typescript-language-server --stdio` | `OCTOCODE_TS_SERVER_PATH` |
| JavaScript | `.js`, `.mjs`, `.cjs` | Yes | `typescript-language-server --stdio` | `OCTOCODE_TS_SERVER_PATH` |
| JSX | `.jsx` | Yes | `typescript-language-server --stdio` | `OCTOCODE_TS_SERVER_PATH` |
| Python | `.py`, `.pyi` | Yes | `pylsp` | `OCTOCODE_PYTHON_SERVER_PATH` |
| Go | `.go` | Yes | `gopls serve` | `OCTOCODE_GO_SERVER_PATH` |
| Rust | `.rs` | Yes | `rust-analyzer` | `OCTOCODE_RUST_SERVER_PATH` |
| Java | `.java` | Yes | `jdtls` | `OCTOCODE_JAVA_SERVER_PATH` |
| C | `.c`, `.h` | Yes | `clangd` | `OCTOCODE_CLANGD_SERVER_PATH` |
| C++ | `.cpp`, `.cc`, `.cxx`, `.hpp` | Yes | `clangd` | `OCTOCODE_CLANGD_SERVER_PATH` |
| C# | `.cs` | Yes | `csharp-ls` | `OCTOCODE_CSHARP_SERVER_PATH` |
| Shell | `.sh`, `.bash`, `.zsh` | Yes | `bash-language-server start` | `OCTOCODE_BASH_SERVER_PATH` |
| JSON / JSONC | `.json`, `.jsonc` | Yes | `vscode-json-language-server --stdio` | `OCTOCODE_JSON_SERVER_PATH` |
| YAML | `.yaml`, `.yml` | Yes | `yaml-language-server --stdio` | `OCTOCODE_YAML_SERVER_PATH` |
| TOML | `.toml` | Yes | `taplo lsp stdio` | `OCTOCODE_TOML_SERVER_PATH` |
| HTML | `.html`, `.htm` | Yes | `vscode-html-language-server --stdio` | `OCTOCODE_HTML_SERVER_PATH` |
| CSS | `.css` | Yes | `vscode-css-language-server --stdio` | `OCTOCODE_CSS_SERVER_PATH` |
| SCSS | `.scss` | Yes | `vscode-css-language-server --stdio` | `OCTOCODE_CSS_SERVER_PATH` |
| Less | `.less` | Yes | `vscode-css-language-server --stdio` | `OCTOCODE_CSS_SERVER_PATH` |

TypeScript and JavaScript use the TypeScript language server. Other languages use their own servers. If a package manager installs `typescript-language-server` as a non-executable `cli.mjs`, Octocode starts it through the current Node executable.

## Live Benchmark Coverage

| Case | Server | Operations verified |
| --- | --- | --- |
| `typescript` | `typescript-language-server` | definition, references, hover, document symbols, type definition, implementation, call hierarchy |
| `javascript` | `typescript-language-server` | definition, references, hover, document symbols, call hierarchy |
| `python` | `pylsp` | definition, references, hover, document symbols |
| `go` | `gopls serve` | definition, references, hover, document symbols, type definition, implementation, call hierarchy |
| `rust` | `rust-analyzer` | definition, references, hover, document symbols, type definition, implementation, call hierarchy |
| `cpp` | `clangd` | definition, references, hover, document symbols, implementation |
| `custom` | fixture server from `benchmark/custom/lsp-servers.json` | definition, references, hover, document symbols |

Unavailable servers are reported as skipped. Unsupported server capabilities are also reported as skipped or empty semantic evidence, not hidden as success.

## Public API

Main entrypoint:

```ts
import {
  LSPClient,
  acquirePooledClient,
  getLanguageServerForFile,
  isLanguageServerAvailable,
  releaseAllPooledClients,
} from 'octocode-lsp';
```

Subpath exports:

| Export | Use |
| --- | --- |
| `octocode-lsp/manager` | Availability checks, pooled clients, cleanup |
| `octocode-lsp/client` | Direct client lifecycle for tests and benchmarks |
| `octocode-lsp/config` | Rust-backed language detection and server config |
| `octocode-lsp/resolver` | Rust-backed symbol-to-position resolver |
| `octocode-lsp/workspaceRoot` | Workspace root inference |
| `octocode-lsp/validation` | Safe file reads and server path validation |
| `octocode-lsp/types` | Shared LSP DTOs |

Tool runtime code should prefer `manager` APIs.

```ts
import { acquirePooledClient } from 'octocode-lsp/manager';
import { resolveWorkspaceRootForFile } from 'octocode-lsp/workspaceRoot';
import type { CodeSnippet } from 'octocode-lsp/types';
```

## Tool Integration Contract

The Octocode tool path is:

```text
octocode-tools-core lspGetSemantics
  -> octocode-lsp public exports
    -> local stdio language server
```

The MCP package registers the tools-core executor; it should not reimplement LSP runtime behavior.

Good integration:

- Resolve symbol anchors with `SymbolResolver.resolvePosition(filePath, fuzzy)`.
- Acquire pooled clients with `acquirePooledClient(workspaceRoot, filePath)`.
- Check `client.hasCapability(...)` before semantic operations.
- Do not call `client.stop()` on pooled clients from tool code.
- Keep paths absolute at the package boundary.

Relevant adapters:

- [tools-core semantic executor](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-tools-core/src/tools/lsp/semantic_content/execution.ts)
- [tools-core symbol anchor resolver](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-tools-core/src/tools/lsp/shared/resolveSymbolAnchor.ts)
- [MCP registration](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-mcp/src/tools/lsp/semantic_content/register.ts)

## Custom Servers

Use custom config for private languages, internal DSLs, or a different server for a known extension.

Lookup order:

1. `OCTOCODE_LSP_CONFIG`
2. `<workspace>/.octocode/lsp-servers.json`
3. `~/.octocode/lsp-servers.json`

Shape:

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

Rules:

- Extension keys include the leading dot.
- `command` is a direct executable name or absolute executable path.
- `args` are process args, not shell-expanded strings.
- Shell commands such as `sh`, `bash`, `zsh`, `cmd.exe`, and `powershell.exe` are rejected.
- `initializationOptions` are forwarded to the LSP `initialize` request.

## Native Packaging

The root package loads a platform package first, then local dev artifacts:

```text
octocode-lsp
  -> octocode-lsp-darwin-arm64
  -> octocode-lsp-linux-x64-gnu
  -> ...
```

Build scripts copy the fresh `.node` artifact into the matching optional package:

```bash
yarn build:dev
```

The copy is handled by [scripts/sync-platform-binaries.cjs](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-lsp/scripts/sync-platform-binaries.cjs).

## Development

Common commands:

```bash
yarn typecheck
yarn lint
yarn test
yarn test:rust
yarn build:dev
yarn benchmark typescript javascript python go rust cpp custom
```

Package quality bar:

- TypeScript typecheck passes.
- ESLint passes.
- Rust `cargo test` and `cargo clippy --all-targets -- -D warnings` pass.
- Vitest coverage remains above 90%.
- Live benchmark has no failed operations and reports p50/p95/p99 latency plus memory deltas.

## Troubleshooting

`No language server is available`

- Confirm the extension appears in the supported-language table.
- Confirm the server command is installed.
- Set the matching `OCTOCODE_*_SERVER_PATH` when the server is not on `PATH`.
- For TypeScript/JavaScript, a non-executable `cli.mjs` install is okay; Octocode runs it through Node.
- For Python on macOS, user installs may live under `~/Library/Python/<version>/bin`.

`Server starts but an operation returns empty`

- Check `client.hasCapability(...)` for that provider.
- Some servers do not support every LSP feature. For example, `pylsp` does not currently provide call hierarchy.
- Run the relevant benchmark case with the same server installed.

`Custom config is ignored`

- Check the extension key includes the leading dot.
- Check the JSON shape under `languageServers`.
- Use direct commands, not shell wrappers.
- Remember the lookup order: env path, workspace config, then user config.
