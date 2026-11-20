# Publishing the native platform packages

`@octocodeai/octocode-native` is a private workspace: it publishes one package per platform in `bin/platform.cjs` (`PLATFORMS`), `@octocodeai/octocode-native-<platform>`, each holding the native binary (file `octocode`, `octocode.exe` on Windows) and no npm bin. The binary runs the CLI commands and the stdio MCP server (`octocode mcp`); the `octocode` and `octocode-mcp` launchers pin every platform package in `optionalDependencies` and start it.

The internal Rust crates compile into that binary; none is published to a Cargo registry.

## Platforms

| Suffix | Rust target |
|---|---|
| `darwin-arm64` | `aarch64-apple-darwin` |
| `darwin-x64` | `x86_64-apple-darwin` |
| `linux-arm64-gnu` | `aarch64-unknown-linux-gnu` |
| `linux-arm64-musl` | `aarch64-unknown-linux-musl` |
| `linux-x64-gnu` | `x86_64-unknown-linux-gnu` |
| `linux-x64-musl` | `x86_64-unknown-linux-musl` |
| `win32-arm64-msvc` | `aarch64-pc-windows-msvc` |
| `win32-x64-msvc` | `x86_64-pc-windows-msvc` |

`PLATFORMS` is the one list. To add or remove a platform, edit it and run `node scripts/platform-packages.cjs`: it writes each `npm/<platform>/package.json` (`os`, `cpu`, `libc`, `files`, `preferUnplugged`) and `README.md`, and both launchers' `optionalDependencies`. `test:scripts` runs it with `--check`.

## Build

```sh
yarn workspace @octocodeai/octocode-native build:dev                  # host, dev profile
yarn workspace @octocodeai/octocode-native build                      # host, release profile
yarn workspace @octocodeai/octocode-native build:target darwin-arm64  # one platform, release
yarn workspace @octocodeai/octocode-native build:all                  # every platform, concurrently (--jobs N)
yarn workspace @octocodeai/octocode-native check:target win32-x64-msvc  # one platform, type-check only
```

`check:target <platform>` (or `node scripts/build-native.cjs --check --all`) type-checks the same selection through the same cross tool, C build scripts included, and neither links nor stages. Use it while editing platform-specific code (`cfg(windows)`, `cfg(target_os = "linux")`): after a runtime edit it takes about 10s, a release target build about 40s (Apple Silicon host, warm `target/platforms/<platform>/`). darwin targets need a macOS host (Apple SDK).

Every script runs `scripts/build-native.cjs`, which builds the `octocode-cli` binary in one Cargo invocation with the committed lockfile, then stages it atomically into `npm/<platform>/`. Staging replaces the file's inode instead of overwriting it in place, so a running MCP server does not crash. Target builds use `target/platforms/<platform>/`; Linux targets cross-link with cargo-zigbuild and zig, Windows with cargo-xwin. Cross-linked gnu targets link against glibc 2.28, Node 24's own floor (`GLIBC_FLOOR` in the script), so the binary runs wherever Node does. Check the floor with `llvm-readelf -V npm/<platform>/octocode`: no `GLIBC_` version above 2.28. A native Linux build links the host's glibc instead. musl targets link statically and allocate through mimalloc (`crates/cli/src/main.rs`), since musl's own allocator serializes threads. Windows targets link the MSVC runtime statically (`+crt-static` in `.cargo/config.toml`), so `octocode.exe` needs no Visual C++ Redistributable. Release builds set `MACOSX_DEPLOYMENT_TARGET` to `MACOS_FLOOR` (11.0) for darwin targets and remap the home, Cargo home and repository paths (`--remap-path-prefix` for Rust, `-ffile-prefix-map` for C outside Windows), so no build-machine path ships. A `RUSTFLAGS` or `CARGO_ENCODED_RUSTFLAGS` variable replaces all configured Rust flags, these included: leave both unset for release builds. When the target is the host, staging runs the binary in a subprocess.

A cross-built artifact is not runtime proof, and no CI runs the binaries: before publishing, run each one on its platform's own OS and CPU, each musl binary in Alpine (`node:24-alpine`) and each gnu binary also in a glibc 2.28 image (`almalinux:8`): `--version` (`octocode <version>`, optionally `+<hex>`, the contract fingerprint), `config --manage` accepting the expected contract fingerprint, `schema`, and one `localSearch` call. `pack-release.cjs` checks each binary's identity (format, CPU, libc with no `GLIBC_` above `GLIBC_FLOOR`, embedded version and fingerprint) before packing.

## Version

The platform packages, the launchers and the Rust workspace (`workspace.package.version` in `Cargo.toml`) share one version. After a version bump, `node scripts/platform-packages.cjs` rewrites the platform manifests and the launchers' exact pins, so a mismatch fails to install instead of selecting another binary. Bump before you build: a binary built before the bump reports the old version.

## Preflight

From the repository root:

```sh
node skills-dev/octocode-dev/scripts/prepublish.mjs
yarn workspace @octocodeai/octocode-native verify
```

Once each binary has passed those runs on its own platform, `node scripts/pack-release.cjs --out release-packages` packs one tarball per `PLATFORMS` entry holding exactly its manifest's files (the binary executable), plus both launchers, into `release-packages/` with `SHA256SUMS`. Platform packages have no `prepublishOnly` hook. Generated-contract provenance records the source package/version, format version, contract fingerprint, and contract hash; it does not attest to a clean Git tree. Run the published-core sync gate and follow the owner commit requirements in the release guide.

## Canary and promotion

Publish a prerelease under a dist-tag first, from `release-packages/` (check `SHA256SUMS` first): the platform packages, then the launchers at the same version.

```sh
for f in release-packages/octocodeai-octocode-native-*-<v>.tgz; do
  npm publish "$f" --access public --tag next
done
npm publish release-packages/octocode-mcp-<v>.tgz --tag next
npm publish release-packages/octocode-<v>.tgz --tag next
```

On every platform, install from the registry, not the workspace, and check:

```sh
npx -y octocode@next --version
npx -y octocode@next schema
```

Also run a real stdio MCP call through `npx -y octocode-mcp@next`. Then promote the platform packages, then the launchers.

## Rollback

npm artifacts are immutable. Record the dist-tags before promotion. If a canary fails, leave `latest` unchanged and publish a corrected prerelease. If a promoted release fails, restore the launchers' dist-tags first, then the platform tags where the exact pins allow. Keep failed artifacts for reproduction.
