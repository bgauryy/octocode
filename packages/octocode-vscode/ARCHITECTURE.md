# VS Code extension architecture

`octocode-mcp-vscode` is a management interface for authentication, MCP process lifecycle, and editor configuration. It does not execute Octocode research tools.

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
          └── process control ──▶ octocode-mcp stdio child
```

## Ownership

- `extension.ts` owns activation, command registration, sign-in and sign-out, status presentation, and disposal.
- `githubAuth.ts` owns the GitHub session lookup through the VS Code authentication API and the manual-token fallback.
- `configPaths.ts` owns supported-client discovery; `mcpConfig.ts` owns configuration updates and token placement; `mcpInstaller.ts` applies them to the editor and every supported client and reports the outcome.
- `mcpProcess.ts` owns the child process and stdio lifecycle.
- `octocode-mcp` and `@octocodeai/octocode-native` own all research behavior.

## Invariants

- The token reaches the MCP server only as `GH_TOKEN`: in the child environment, and in the `env` block of the Octocode entry it writes (owner-only files, written atomically). Older `GITHUB_TOKEN`/`OCTOCODE_TOKEN` entries are removed.
- The child is `npx -y octocode-mcp@latest` with fixed arguments; Windows runs it through the shell.
- Configuration writes preserve unrelated client entries and are scoped to explicitly supported targets.
- Activation and repeated install commands are idempotent.
- Process shutdown releases listeners and child resources.
- Failure to locate or launch MCP is surfaced to the user; the extension never substitutes a tool implementation.
