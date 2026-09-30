# Publishing `@octocodeai/octocode-native`

This package owns one distribution for four artifacts on six platforms:

- `octocode` native CLI;
- `octocode-regex-worker`;
- `NativeRuntime` addon exposed by `.` and `./runtime`;
- engine primitive addon exposed by `./engine`.

Five internal Rust crates compile into the same four artifacts. Runtime and engine remain separate Node addons; no Rust crate is published to a Cargo registry.

## Package map

```text
packages/octocode-native/
├── Cargo.toml
├── crates/runtime/             pure runtime library
├── crates/cli/                 CLI and regex worker
├── crates/runtime-napi/        runtime Node adapter
├── crates/github/              GitHub protocol services
├── crates/engine/              primitives and engine Node adapter
├── js/                         runtime and engine loaders/types
├── bin/                        Node launchers
├── npm/<platform>/             four release artifacts per platform
├── scripts/                    build, copy, ABI, size, and version checks
└── package.json
```

Supported platform suffixes are:

| Suffix | Rust target |
|---|---|
| `darwin-arm64` | `aarch64-apple-darwin` |
| `darwin-x64` | `x86_64-apple-darwin` |
| `linux-arm64-gnu` | `aarch64-unknown-linux-gnu` |
| `linux-x64-gnu` | `x86_64-unknown-linux-gnu` |
| `linux-x64-musl` | `x86_64-unknown-linux-musl` |
| `win32-x64-msvc` | `x86_64-pc-windows-msvc` |

## Build

For the current host during development:

```sh
yarn workspace @octocodeai/octocode-native build:dev
```

Every build script runs `scripts/build-native.cjs`. It builds both host crates
in one Cargo invocation and, concurrently, the engine addon in its own target dir
(`target/napi-engine/`; the engine needs different features, and Cargo holds one
lock per target dir). It then stages atomically: CLI binaries into
`npm/<platform>/`, both addons into the package root. `build` is the same in
release mode; `build:hosts:dev` / `build:engine:dev` build one half and
`--serial` restores the one-dir sequential flow for comparison.

For one release target:

```sh
yarn workspace @octocodeai/octocode-native build:target darwin-arm64
```

The platform argument is one of `darwin-arm64`, `darwin-x64`, `linux-arm64-gnu`, `linux-x64-gnu`, `linux-x64-musl`, `win32-x64-msvc`.

For a complete release matrix (platforms build concurrently, each in
`target/platforms/<platform>/`; Linux targets cross-link with cargo-zigbuild and
Windows with cargo-xwin, mirroring napi's `--cross-compile`):

```sh
yarn workspace @octocodeai/octocode-native build:all
yarn workspace @octocodeai/octocode-native platforms:check
```

`build:target <platform>` produces and stages all four artifacts using the committed lockfile. The internal runtime adapter library is named `octocode_runtime_napi`; staging preserves the published `octocode-native.<platform>.node` filename. Engine filenames and capability features remain unchanged. Darwin staging replaces linker-generated ad-hoc addon signatures with fresh ad-hoc signatures. When the target matches the host, staging loads both addons in subprocesses immediately. `platforms:check` verifies all 24 files and also loads both host-platform addons.

Cross-target presence is not runtime proof. CI builds no native artifacts; runtime proof comes from running `node ../verify-binary.cjs` inside `npm/<platform>/` on a machine of that platform, and the 24-file `platforms:check` gate runs after `build:all` (part of `dev.mjs build:publish`).

## Version contract

The coordinator, all six platform packages, and all five internal Rust crates use one version. Rust crates inherit `workspace.package.version`; Cargo metadata supplies the membership and resolved versions for validation.

```sh
yarn workspace @octocodeai/octocode-native version:sync
yarn workspace @octocodeai/octocode-native version:check
```

Run version synchronization before building release artifacts. It updates the shared Cargo version and npm manifests, then resolves the updated workspace against the existing lockfile offline instead of broadly regenerating it. Review any dependency changes; offline mode prevents fetching but can still select cached versions. Review those changes and rebuild. Prepublish runs the read-only version check; it never synchronizes versions or regenerates a lockfile. Do not hand-publish mixed root/platform versions: exact optional dependencies intentionally turn a mismatch into a loader failure rather than silently selecting another ABI.

## Preflight

From the repository root:

```sh
node skills-dev/octocode-dev/scripts/prepublish.mjs
yarn workspace @octocodeai/octocode-native verify
yarn workspace @octocodeai/octocode-native pack:check
yarn workspace @octocodeai/octocode-native platforms:check
```

Required evidence:

- Rust crate-boundary checks, formatting, clippy, type checks, and tests pass (including GitHub protocol, CLI, and runtime adapter tests);
- Node runtime and engine tests pass;
- loader and N-API ABI checks pass;
- all 24 artifacts exist;
- host addons load independently;
- each platform package’s `npm/verify-binary.cjs` passes on its matching runner;
- dry-run tarballs contain only intended release files.

The generated-contract provenance test also requires a clean, current Core source receipt. Do not regenerate a provenance receipt merely to hide a dirty source tree.

## Publish order

Use a prerelease version and dist-tag first.

1. Publish all six platform packages.
2. Publish `@octocodeai/octocode-native` at the exact same version.
3. Install from the registry in clean platform-specific jobs.
4. Publish canary CLI and MCP consumers with exact native dependencies.
5. Promote platform packages, then the root package, then consumers.

Example platform loop after all platform artifacts have been collected and verified:

```sh
for platform in darwin-arm64 darwin-x64 linux-arm64-gnu linux-x64-gnu linux-x64-musl win32-x64-msvc; do
  npm publish "packages/octocode-native/npm/$platform" --access public --tag next
done
npm publish packages/octocode-native --access public --tag next
```

Every platform package has a `prepublishOnly` hook that checks artifact presence and, on its matching host, loads both addons and exercises CLI operations. Do not bypass lifecycle scripts.

## Registry acceptance

On every supported platform, install from the registry rather than from the workspace and verify:

```sh
node -e "const n=require('@octocodeai/octocode-native/runtime'); const r=new n.NativeRuntime(); console.log(r.abiVersion); r.close()"
node -e "const e=require('@octocodeai/octocode-native/engine'); console.log(typeof e.minifyContent)"
npx @octocodeai/octocode-native@next --version
npx @octocodeai/octocode-native@next scheme --compact
```

Also start the packaged regex worker and run direct CLI, Node launcher, and real stdio MCP smoke paths.

## Rollback

npm artifacts are immutable. Record previous dist-tags before promotion. If a canary fails, leave `latest` unchanged and publish a corrected prerelease. If a promoted release fails, first restore consumer dist-tags, then restore the native root and platform tags where dependency constraints permit. Keep failed artifacts available for reproduction.

Pre-consolidation `@octocodeai/octocode-engine` versions remain on npm for old consumers, but this repository no longer publishes that package. New engine consumers use `@octocodeai/octocode-native/engine`.
