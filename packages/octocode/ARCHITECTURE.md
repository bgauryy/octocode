# Octocode CLI architecture

`octocode` is the public Node launcher, installer, and interactive management package. Public tool execution is owned by the Rust runtime in [`../octocode-native`](../octocode-native), not by this package or tools-core.

## Runtime boundary

`src/cli/index.ts` decides whether a command is native-owned before entering the legacy TypeScript dispatcher. `src/cli/native-delegate.ts` resolves `@octocodeai/octocode-native/bin/octocode.cjs` and delegates with inherited stdio and environment.

For every public tool command, the native path is mandatory:

```text
npx octocode → Node launcher → native CLI → Rust ToolRuntime
```

If the platform native package cannot be resolved, the Node launcher fails closed. It must not run the retired TypeScript tool implementation.

The Rust runtime owns contract validation, availability, security, GitHub and artifact providers, local/AST/LSP behavior, bulk orchestration, pagination, response shaping, cancellation, and exit classification. `@octocodeai/octocode-core` owns public schemas and instructions.

## TypeScript-owned management seams

TypeScript remains only where Node or an interactive terminal is part of the feature:

- `skill`, which uses the shared skill installer and must not delegate recursively;
- interactive `install` without `--ide`;
- OAuth/menu and credential-management presentation still reached by interactive flows;
- supporting terminal, platform, MCP-config, and filesystem utilities.

Flag-only native management commands—including direct `tools`, `context`, `lsp-server`, status, MCP installation, and human search/read/AST/LSP commands—belong to the native CLI when selected by the dispatcher.

## Migration debt

`src/cli/tool-command/` and `src/cli/remote-local/materialize.ts` still contain the pre-cutover direct-tool path and import `@octocodeai/octocode-tools-core/direct`. They are not valid fallback execution paths. Remove them after the remaining management callers are separated from legacy routing and remote materialization uses native execution or a purpose-built Rust command.

The CLI also imports focused tools-core utility subpaths for credentials, platform paths, and filesystem presentation. Those utility imports do not grant tools-core ownership of public tool execution and should move to narrower owners over time.

## Build and packaging

- `build.mjs` bundles `src/index.ts` to `out/octocode.js` as an ESM launcher.
- `@octocodeai/octocode-native` is a runtime dependency; its optional platform packages supply the native binary and N-API addon.
- `@octocodeai/octocode-core` remains external for public contracts.
- The build fails when a bare external import is not declared.
- `__APP_VERSION__` is injected from `package.json`.

Publish native platform packages, the native root, contract/config prerequisites, and then this launcher. Exercise the built Node launcher against the packaged native binary before publishing.

## Rules

- Public tool behavior belongs in Rust.
- The Node launcher delegates or fails closed; it never falls back to TypeScript execution.
- Keep management-only TypeScript paths explicit and small.
- Do not add Node-side query batching, provider behavior, security policy, response shaping, or tool-specific error recovery.
- Remove unreachable legacy tool-command code and tools-core direct imports instead of maintaining parallel behavior.
- Validate both the direct native CLI and the built Node launcher, including CLI/MCP structured-result parity.
