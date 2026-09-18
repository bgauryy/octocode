# Publishing `@octocodeai/octocode-native`

This package owns one distribution for four artifacts on six platforms:

- `octocode` native CLI;
- `octocode-regex-worker`;
- `NativeRuntime` addon exposed by `.` and `./runtime`;
- engine primitive addon exposed by `./engine`.

The runtime and engine remain separate Rust crates and separate Node addons.

## Package map

```text
packages/octocode-native/
├── Cargo.toml
├── crates/runtime/
├── crates/engine/
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

For one release target:

```sh
yarn workspace @octocodeai/octocode-native build:darwin-arm64
```

For a complete release matrix:

```sh
yarn workspace @octocodeai/octocode-native build:all
yarn workspace @octocodeai/octocode-native platforms:check
```

`build:<platform>` produces and stages all four artifacts. Darwin staging replaces linker-generated ad-hoc addon signatures with fresh ad-hoc signatures. When the target matches the host, staging loads both addons in subprocesses immediately. `platforms:check` verifies all 24 files and also loads both host-platform addons.

Cross-target presence is not runtime proof. CI must run each `build:<platform>` command on the matching runner, as configured by `.github/workflows/rust-tools-core.yml`, so `npm/verify-binary.cjs` can execute the package’s addons and binaries.

## Version contract

The coordinator and all six platform packages use one version. Rust crate versions are checked against that release line where applicable.

```sh
yarn workspace @octocodeai/octocode-native version:sync
yarn workspace @octocodeai/octocode-native version:check
```

Review the resulting manifest changes. Do not hand-publish mixed root/platform versions: exact optional dependencies intentionally turn a mismatch into a loader failure rather than silently selecting another ABI.

## Preflight

From the repository root:

```sh
node scripts/prepublish.mjs
yarn workspace @octocodeai/octocode-native verify
yarn workspace @octocodeai/octocode-native pack:check
yarn workspace @octocodeai/octocode-native platforms:check
```

Required evidence:

- Rust formatting, clippy, type checks, and tests pass;
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
4. Publish canary CLI, MCP, and Pi consumers with exact native dependencies.
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
npx @octocodeai/octocode-native@next tools --json
```

Also start the packaged regex worker and run direct CLI, Node launcher, and real stdio MCP smoke paths.

## Rollback

npm artifacts are immutable. Record previous dist-tags before promotion. If a canary fails, leave `latest` unchanged and publish a corrected prerelease. If a promoted release fails, first restore consumer dist-tags, then restore the native root and platform tags where dependency constraints permit. Keep failed artifacts available for reproduction.

Pre-consolidation `@octocodeai/octocode-engine` versions remain on npm for old consumers, but this repository no longer publishes that package. New engine consumers use `@octocodeai/octocode-native/engine`.
