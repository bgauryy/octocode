# LSP lifecycle and provisioning

For the public query contract, see [`lspSearch`](../../../../docs/OCTOCODE_TOOLS.md#lspsearch). For grammar and feature coverage, see [Supported languages and features](SUPPORTED_LANGUAGES_AND_FEATURES.md).

## Semantic boundary

Tree-sitter and Oxc answer syntactic questions from embedded parsers. LSP operations launch a real language server over stdio to resolve cross-file identity, definitions, references, types, implementations, and call relationships. These evidence classes are not interchangeable.

When an operation requires an unavailable server, `lspSearch` returns an error row with `errorCode:"lspServerUnavailable"` and valid recovery calls. It never labels same-file or syntactic guesses as semantic results. A running server that lacks the requested capability returns `unsupportedOperation`. A supported server returning no rows establishes only an empty result within that server's indexed scope and configuration.

`documentSymbols` may use native Oxc or Markdown outline support without a server. Other semantic operations require their corresponding negotiated LSP capability.

Public positions are one-based and servers communicate in UTF-16. A server selecting an unsupported encoding fails startup.

## Resolution

The Rust engine owns command discovery and launch configuration. For a known extension it resolves, in priority order:

1. a language-specific `OCTOCODE_*_SERVER_PATH` override;
2. an explicit `$OCTOCODE_LSP_CONFIG` entry;
3. `<workspace>/.octocode/lsp-servers.json` only when `OCTOCODE_TRUST_PROJECT_LSP_CONFIG=true`;
4. `~/.octocode/lsp-servers.json`;
5. a known command in the workspace package tree, ecosystem location, managed cache, or `PATH`.

Project configuration is untrusted by default because it controls executable startup. Generic interpreter eval forms such as `node -e` or `python -c` are rejected even when project configuration is trusted. Absolute commands must exist, be regular executable files, and must not be shell wrappers.

The built-in routing table includes:

| Files | Command | Override |
|---|---|---|
| TypeScript / JavaScript | `typescript-language-server --stdio` | `OCTOCODE_TS_SERVER_PATH` |
| Python | `pylsp` | `OCTOCODE_PYTHON_SERVER_PATH` |
| Rust | `rust-analyzer` | `OCTOCODE_RUST_SERVER_PATH` |
| Go | `gopls serve` | `OCTOCODE_GO_SERVER_PATH` |
| Java | `jdtls` | `OCTOCODE_JAVA_SERVER_PATH` |
| C / C++ | `clangd` | `OCTOCODE_CLANGD_SERVER_PATH` |
| C# | `csharp-ls` | `OCTOCODE_CSHARP_SERVER_PATH` |
| Scala (`scala`, `sc`, `sbt`) | `metals` | `OCTOCODE_SCALA_SERVER_PATH` |

Known routing does not imply that a server is installed. The engine npm package no longer bundles JavaScript language servers. Install servers in the workspace or toolchain, expose them on `PATH`, set an override, or use native managed provisioning where supported. `tsgo` may be selected explicitly through `OCTOCODE_TS_SERVER_PATH`, but it is not automatically preferred before the held-out operation matrix establishes parity.

## Managed provisioning

The native CLI owns a pinned managed manifest for `rust-analyzer` and `clangd`:

```bash
octocode lsp-server list
octocode lsp-server status path/to/file.ts
octocode lsp-server install rust-analyzer --yes
octocode lsp-server install clangd --yes
```

Managed installation is explicit. `OCTOCODE_LSP_AUTO_INSTALL=off|prompt|auto` controls whether an install command may fetch without additional confirmation; normal `lspSearch` execution does not download executables.

Provisioning permits only allowed HTTPS hosts, follows the same restriction across redirects, requires a pinned SHA-256, supports bounded `none`, `gz`, and `zip` extraction, uses per-target locks, and atomically publishes a completion-marked executable. Unsupported platforms fail with an installation hint.

## Custom configuration

A configuration maps file extensions to launch specs:

```json
{
  "languageServers": {
    ".php": {
      "command": "intelephense",
      "args": ["--stdio"],
      "languageId": "php"
    }
  }
}
```

`command` and `languageId` are required. `args` defaults to an empty array and `initializationOptions` is passed to `initialize`. The example restores an explicit user-owned route for a language with no built-in route; it does not make PHP first-class. A custom entry overrides built-in routing for that extension but does not bypass executable validation.

## Pool ownership

`packages/octocode-native/crates/engine/src/lsp/pool.rs` owns the complete pool lifecycle. One key identifies the server command, arguments, workspace, initialization options, and effective environment.

- Concurrent starts and health checks deduplicate.
- Active requests prevent idle shutdown.
- Successful use renews idle expiry and drives least-recently-used eviction.
- Clearing a key invalidates pending starts and waiters; late completions cannot publish stale clients.
- Failed health checks evict the client so the next acquisition starts a replacement.
- Runtime shutdown stops every client and joins owned work.

Long-lived MCP sessions can reuse warm servers. A one-shot native CLI process cannot share its pool with a later process.

## Readiness and diagnostics

The JSON-RPC transport bounds frames, writes, notifications, stderr retention, and cancellation. Timed-out requests emit `$/cancelRequest` when possible.

Progress-aware servers must reach a full quiet interval before readiness is confirmed. New progress restarts that interval. Servers without progress use a bounded settle state, which does not prove indexing completion. Empty results with unconfirmed readiness remain partial.

Pull-capable servers use `textDocument/diagnostic`. Push diagnostics are retained in a bounded per-document cache; content updates clear stale entries, versions reject older publications, and notifications for another document cannot satisfy a waiter.

Semantic results are sorted and deduplicated before pagination. Continuations bind to the query, server identity, workspace/configuration fingerprints, and result snapshot. A changed result set produces a restart rather than combining pages from different semantic states.

## Rust context

The public `rustContext` fields map natively to rust-analyzer initialization options. They control Cargo features, default features, target, cfg values, build scripts, and procedural macros. Procedural macros require build scripts. Enabling either permits workspace code execution by rust-analyzer; Octocode does not sandbox that execution.

Effective Rust context participates in pool and continuation identity, so clients with different build configurations cannot share semantic state.
