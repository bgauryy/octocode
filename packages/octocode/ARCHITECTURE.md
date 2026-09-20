# Octocode CLI architecture

`octocode` is the public Node launcher, installer, and interactive management package. All public tool execution is owned by the Rust runtime in [`../octocode-native`](../octocode-native).

## Runtime boundary

`src/cli/index.ts` is a three-branch launcher: `skill` runs in Node, a bare TTY `install` opens the client picker, and every other argv is forwarded verbatim — parsing, help, version, and validation included — to the native binary. `src/cli/native-delegate.ts` resolves `@octocodeai/octocode-native/bin/octocode.cjs` and delegates with inherited stdio and environment. There is no TypeScript command registry, help renderer, or option validation for native-owned commands.

```text
npx octocode → Node launcher → native CLI → Rust ToolRuntime
```

The native path is mandatory for public tools. If its platform package cannot be resolved, the launcher fails closed. Rust owns contract validation, availability, security, GitHub and artifact providers, local/AST/LSP behavior, bulk orchestration, pagination, response shaping, cancellation, and exit classification. `@octocodeai/octocode-core` owns public schemas and instructions.

## TypeScript management surface

TypeScript remains only for:

- `skill`, backed by the shared skill installer;
- the TTY picker for `install` without `--ide`, which discovers client ids from `native install --list --json` and delegates the selected id back to native.

Everything else — flag-only management commands, `tools`, `context`, `lsp-server`, status, authentication, human search/read/AST/LSP commands, and MCP installation — is delegated to the native CLI. Interactive installation is a transport adapter, not a second installer. The native `skill` command shells back to this launcher; `OCTOCODE_SKILL_DELEGATED` guards that hop so a native binary on PATH cannot recurse.

## Build and packaging

- `build.mjs` bundles `src/index.ts` to `out/octocode.js` as an ESM launcher.
- `@octocodeai/octocode-native` is a runtime dependency; its optional platform packages supply the native binary and N-API addon.
- `@octocodeai/octocode-core` remains external for public contracts.
- The build fails when a bare external import is not declared.
- `__APP_VERSION__` is injected from `package.json`.

Publish native platform packages, the native root, contract/config prerequisites, and then this launcher. Exercise the built Node launcher against the packaged native binary before publishing.

## Rules

- Public tool behavior belongs in Rust.
- The Node launcher delegates or fails closed; it has no TypeScript tool fallback.
- Keep management-only TypeScript paths explicit and small.
- Do not add Node-side query batching, provider behavior, security policy, response shaping, or tool-specific error recovery.
- Validate both the direct native CLI and the built Node launcher, including CLI/MCP structured-result parity.
