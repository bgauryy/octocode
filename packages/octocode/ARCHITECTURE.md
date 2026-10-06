# Octocode CLI architecture

`octocode` is the public Node launcher, installer, and interactive management package. All public tool execution is owned by the Rust runtime in [`../octocode-native`](../octocode-native).

## Runtime boundary

`src/cli/index.ts` owns catalog presentation, skill materialization, and the bare TTY install picker. Bare invocation and `scheme` compose core presentation with native availability. Root `--help`, `-h`, and `help` append canonical agent instructions after native help. `config view` starts the bundled local server with a fixed native JSON transport. Other argv is forwarded verbatim—parsing, subcommand help, version, and validation included—to the native binary. `src/cli/native-delegate.ts` resolves `@octocodeai/octocode-native/bin/octocode.cjs` and delegates with inherited stdio and environment. There is no TypeScript execution registry or fallback for native-owned commands.

```text
npx octocode → Node launcher → native CLI → Rust ToolRuntime
```

The native path is mandatory for public tools. If its platform package cannot be resolved, the launcher fails closed. Rust owns contract validation, availability, security, GitHub and artifact providers, local/AST/LSP behavior, bulk orchestration, pagination, response shaping, cancellation, and exit classification. `@octocodeai/octocode-core` authors public schemas and instructions; the CLI reads them through `@octocodeai/config` (`./schema`, `./mcp`), never from core directly.

## TypeScript management surface

TypeScript remains only for:

- `scheme`, which joins the core public catalog with native availability after
  a fail-closed contract-fingerprint check; it presents contracts but does not
  validate or execute tool requests;
- bare output and root help, which expose the same core-owned agent instructions
  once per output, scoped to CLI availability (including CLI-only `ghCloneRepo`);
- `config view`, which lazily loads `src/cli/config-view/server.ts`; fixed native `config --manage` requests carry the expected contract fingerprint before any mutation;
- `skill`, backed by the shared skill installer;
- the TTY picker for `install` without `--ide`, which discovers client ids from `native install --list --json` and delegates the selected id back to native.

Everything else—`showConfig`, other `config` operations, `auth`, `lsp-server`, tool invocations (`<toolName> '<json>'`), and non-interactive `install`—is delegated to the native CLI. Interactive installation is a transport adapter, not a second installer. The native `skill` command shells back to this launcher; `OCTOCODE_SKILL_DELEGATED` guards that hop so a native binary on PATH cannot recurse.

## Build and packaging

- `build.mjs` bundles `src/index.ts` to `out/octocode.js` as an ESM launcher.
- `@octocodeai/octocode-native` is a runtime dependency; its optional platform packages supply the native binary and N-API addon.
- `@octocodeai/config` (bundled into the tarball via `bundledDependencies`) supplies public contracts and instructions; the skill installer is inlined at build time.
- `src/cli/config-view/` holds the temporary authenticated loopback server; its browser assets (`assets/`) are inlined as strings through `?raw` imports (Vite in tests, the `raw-text` esbuild plugin in `build.mjs`). Native owns paths, field policy, secret redaction, agent adapters, and file writes.
- The build fails when a bare external import is not declared.
- `__APP_VERSION__` is injected from `package.json`.

Publish native platform packages, the native root, contract/config prerequisites, and then this launcher. Exercise the built Node launcher against the packaged native binary before publishing.

## Rules

- Public tool behavior belongs in Rust.
- Public contract content belongs in core (delivered via config); `scheme` only reconciles it
  with native availability and the enforcement fingerprint.
- The Node launcher delegates or fails closed; it has no TypeScript tool fallback.
- Keep management-only TypeScript paths explicit and small.
- Do not add Node-side query batching, provider behavior, security policy, response shaping, or tool-specific error recovery.
- Validate both the direct native CLI and the built Node launcher, including CLI/MCP structured-result parity.

Global `.env` mutation uses the native config module’s shared parser and protection policy. The CLI accepts key/value arguments or a value on stdin, prints only mutation metadata, and delegates locked atomic file replacement to config.
