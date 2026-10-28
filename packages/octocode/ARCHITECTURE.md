# Octocode CLI architecture

`octocode` is the npm launcher for the native Rust CLI in [`../octocode-native`](../octocode-native). Native owns every command, including help, version, skills, interactive installation, and the browser configuration editor.

## Runtime boundary

`src/cli/index.ts` forwards the argument vector unchanged. `src/cli/native-delegate.ts` resolves the platform binary through the native package's shared resolver and runs it with inherited stdin, stdout, stderr, environment, and working directory. The shared `bin/launch-native.cjs` forwards signals and returns the child's exit status. Node does not parse flags, select commands, format native output, or launch a separate management implementation.

```text
npx octocode → Node launcher → native CLI → Rust ToolRuntime
```

`OCTOCODE_NATIVE_BIN` is a development override governed by the shared config rule and ignored in production. If resolution or process startup fails, Node prints the launcher failure on stderr and exits `5`; it does not interpret output flags. Once native starts, its stdout, stderr, and exit status pass through unchanged.

Rust owns Clap parsing, catalog presentation, validation, config and authentication, client selection and installation, embedded skills, the temporary config-view server, and every research tool. Core authors public tool definitions; config generates the artifacts that native embeds.

## Build and packaging

- `build.mjs` bundles `src/index.ts` to `out/octocode.js` as an ESM launcher.
- `octocode` declares every native platform package in `optionalDependencies` (written from `PLATFORMS` by `packages/octocode-native/scripts/platform-packages.cjs`); npm installs the one matching `os`/`cpu`/`libc`. The resolver, the registry fallback and the signal-forwarding launcher from `packages/octocode-native/bin/` are bundled at build time.
- When npm skipped the platform package, the bundled launcher fetches it once at the pinned version (`__OCTOCODE_NATIVE_VERSION__`) into `~/.octocode/native/` (`OCTOCODE_HOME`), verifying the registry's sha512 and the binary's `--version`. Unbundled source runs never download.
- The bundle targets Node 22 (`engines.node` `>=22`); the repository's own toolchain stays on Node 24.
- `@typescript/native-preview` supplies the packaged `tsgo` language server used by native `lspSearch`; it is not a CLI parser.
- The private `@octocodeai/config` root export is bundled for the development-override policy. The launcher does not import public tool contracts or instructions.
- `scripts/stage-skills.mjs` stages canonical public skills for the npm package and native build. The native binary embeds the same publishable skill directories.
- The build rejects undeclared external runtime imports.

Publishing order and checks: [release guide](../../skills-dev/octocode-dev/docs/RELEASE.md).

## Invariants

- CLI behavior and presentation belong in native; Node owns process startup only.
- Public tool definitions belong in core and runtime behavior belongs in Rust.
- Keep arguments, stdio, environment, working directory, signals, and exit status intact across the launcher boundary.
- Verify direct native and built npm entry points against the same command cases. Browser session ports and credentials are generated per invocation; their lifecycle and API contract must match.
