# VS Code extension architecture

`octocode-mcp-vscode` is a management interface for authentication, MCP process lifecycle, and editor configuration. It does not execute Octocode research tools. It also publishes an IDE bridge: a loopback endpoint through which the native `lspSearch` asks this editor's own language providers.

## Data flow

```text
VS Code command / activation
          │
          ├── GitHub sign-in ──▶ VS Code authentication session
          │                         │
          │                         ├── GH_TOKEN for the managed MCP process
          │                         └── GH_TOKEN in each client's octocode env block
          │
          ├── client detection ──▶ validated MCP configuration mutation
          │
          ├── process control ──▶ octocode-mcp stdio child
          │
          └── octocode.ideBridge.enabled ──▶ 127.0.0.1 HTTP server + ~/.octocode/ide/<pid>.json
                                                │
                native lspSearch ──(Bearer token)┘──▶ vscode.execute*Provider / prepare*Hierarchy / getDiagnostics
```

## Ownership

- `extension.ts` owns activation, command registration, sign-in and sign-out, status presentation, and disposal.
- `githubAuth.ts` owns the GitHub session lookup through the VS Code authentication API and the manual-token fallback.
- `configPaths.ts` owns supported-client discovery; `mcpConfig.ts` owns configuration updates and token placement; `mcpInstaller.ts` applies them to the editor and every supported client and reports the outcome.
- `mcpProcess.ts` owns the child process and stdio lifecycle.
- `ideBridge.ts` owns the IDE bridge server: routing, Host/token checks, provider calls through the built-in commands, and the hierarchy-item handles. `ideBridgeMethods.ts` owns the method table and request validation, `lspJson.ts` the VS Code to LSP JSON conversion, `ideBridgeHttp.ts` the body limit and constant-time token compare, `ideBridgeLock.ts` the owner-only lock file, and `ideBridgeController.ts` starts and stops the bridge as `octocode.ideBridge.enabled` changes. The wire protocol is the RFC's "Rung 2: IDE bridge"; the native runtime owns the client, discovery and path authorization.
- `octocode-mcp` and `@octocodeai/octocode-native` own all research behavior.

## Invariants

- The token reaches the MCP server only as `GH_TOKEN`: in the child environment, and in the `env` block of the Octocode entry it writes (owner-only files, written atomically). Older `GITHUB_TOKEN`/`OCTOCODE_TOKEN` entries are removed.
- The child is `npx -y octocode-mcp@latest` with fixed arguments; Windows runs it through the shell.
- Configuration writes preserve unrelated client entries and are scoped to explicitly supported targets.
- Activation and repeated install commands are idempotent.
- Process shutdown releases listeners and child resources.
- Failure to locate or launch MCP is surfaced to the user; the extension never substitutes a tool implementation.
- The bridge listens on `127.0.0.1` only, on an OS-assigned port. Every request needs `Host: 127.0.0.1:<port>` (DNS-rebinding guard) and `Authorization: Bearer <token>` (32 random bytes, hex, fresh per start, compared in constant time and never echoed or logged); bodies are JSON of at most 1 MiB.
- The lock file `<Octocode home>/ide/<pid>.json` (`OCTOCODE_HOME` or `~/.octocode`) is the only place the token is stored: directory `0700`, file `0600`, written by temp file and rename, rewritten when workspace folders change, removed on stop, dispose and deactivate.
- The bridge runs only the editor's own providers through built-in commands; it executes no repository code, starts no server and implements no research tool. Native applies path authorization and redaction to every answer.
- Hierarchy items are kept in a per-bridge map of at most 1024 handles (oldest evicted); an unknown or evicted handle answers 404 with a reason, never an empty result.
- An empty answer is trusted only when the document's language is registered and its document-symbol provider answers; otherwise the bridge answers `{"unsupported": true}` so native falls to a server instead of claiming absence.
