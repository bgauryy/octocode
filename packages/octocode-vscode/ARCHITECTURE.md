# VS Code extension architecture

`octocode-mcp-vscode` is a management interface for authentication, MCP process lifecycle, and editor configuration. It does not execute Octocode research tools.

## Data flow

```text
VS Code command / activation
          │
          ├── GitHub OAuth ──▶ VS Code secret storage
          │                         │
          │                         └── token environment for managed MCP process
          │
          ├── client detection ──▶ validated MCP configuration mutation
          │
          └── process control ──▶ octocode-mcp stdio child
```

## Ownership

- `extension.ts` owns activation, command registration, status presentation, and disposal.
- Authentication modules own GitHub OAuth and secret-storage interaction.
- Installer modules own supported-client discovery and configuration updates.
- `mcpProcess.ts` owns the child process and stdio lifecycle.
- `octocode-mcp` and `@octocodeai/octocode-native` own all research behavior.

## Invariants

- Tokens are stored through VS Code secret storage and passed through environment variables, never written into generated MCP configuration.
- Child processes use argument arrays rather than shell interpolation.
- Configuration writes preserve unrelated client entries and are scoped to explicitly supported targets.
- Activation and repeated install commands are idempotent.
- Process shutdown releases listeners and child resources.
- Failure to locate or launch MCP is surfaced to the user; the extension never substitutes a tool implementation.
