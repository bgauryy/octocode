# Release Guide

> Release checklist for npm packages, native platform packages, and standalone binaries.

## Contents

- [Overview](#overview)
- [Release Checklist](#release-checklist)
- [What to Publish](#what-to-publish)
- [Runtime Packaging](#runtime-packaging)
- [Release Procedures](#release-procedures)
  - [CI](#ci)
  - [Manual npm Publish](#manual-npm-publish)
  - [Homebrew Tap](#homebrew-tap)
  - [Manual Binary Releases](#manual-binary-releases)
- [Native Build Commands](#native-build-commands)
- [Local Development Build](#local-development-build)
- [References](#references)

## Overview

Octocode ships in three forms:

| Distribution | Used by | Release path |
|---|---|---|
| npm package (`octocode-mcp`) | MCP clients via `npx` or global install | npm publish |
| npm package (`octocode-cli`) | Terminal users, scripts, Homebrew formula | npm publish, then tap update |
| Standalone binaries | Direct downloads, embedding, Pi installs | GitHub Release assets |

The `octocode-mcp` npm package stays small by resolving native runtime assets through platform-specific `optionalDependencies`. The `octocode-cli` npm package is the Homebrew install boundary: it must contain or declare every runtime dependency needed for the `octocode` command. Standalone binaries are built separately and must include the native runtime files they need at execution time.

Releases are manual. Use this guide to publish npm packages, update the Homebrew tap, build standalone binaries, and attach release assets.

## Release Checklist

```text
[ ] Version bumped in every package that will be published.
[ ] Dependent package ranges updated when required security/minifier ranges change.
[ ] Final PR validation is green, or `yarn verify` passes locally.
[ ] Release notes are ready in the GitHub Release draft.
[ ] npm auth is ready: `npm whoami`.
[ ] GitHub Release draft exists for tag vX.Y.Z.
[ ] Verify npm package: https://www.npmjs.com/package/octocode-mcp
[ ] If CLI changed, verify npm package: https://www.npmjs.com/package/octocode-cli
[ ] Verify npm provenance is visible for published packages.
[ ] If standalone binaries changed, upload 6 binary assets plus `checksums-sha256.txt`.
[ ] Smoke test npm: `npx octocode-mcp@latest --help`
[ ] If CLI changed, update and smoke test the Homebrew tap.
[ ] Smoke test at least one downloaded release binary.
```

## What to Publish

Every published package change needs a version bump for that package. If another published package must change its dependency range to consume that version, bump and publish the dependent package too.

| Changed package | Publish | Also update |
|---|---|---|
| `octocode-security` Rust/native code | 6 `octocode-security-*` platform packages, then `octocode-security` | Main package `optionalDependencies`; `octocode-mcp` only if its range changes |
| `octocode-security` TS/JS only | `octocode-security` | `octocode-mcp` only if its range changes |
| `@octocodeai/octocode-context-utils` Rust/native code | 6 `@octocodeai/octocode-context-utils-*` platform packages, then `@octocodeai/octocode-context-utils` | Main package `optionalDependencies`; `octocode-mcp` only if its range changes |
| `@octocodeai/octocode-context-utils` JS/package metadata only | `@octocodeai/octocode-context-utils` | `octocode-mcp` only if its range changes |
| `octocode-mcp` | `octocode-mcp` | Nothing else |
| `octocode-cli` | `octocode-cli` | Homebrew tap formula after npm publish |

Version bump locations:

```text
packages/octocode-security/package.json
packages/octocode-security/npm/*/package.json
packages/octocode-context-utils/package.json
packages/octocode-context-utils/npm/*/package.json
packages/octocode-mcp/package.json
packages/octocode-cli/package.json
```

Native platform package directories:

```text
darwin-arm64
darwin-x64
linux-x64-gnu
linux-x64-musl
linux-arm64-gnu
win32-x64-msvc
```

Publish order is fixed because npm must resolve platform `optionalDependencies` after they already exist:

```text
1. 6 octocode-security platform packages
2. octocode-security
3. 6 @octocodeai/octocode-context-utils platform packages
4. @octocodeai/octocode-context-utils
5. octocode-mcp
```

Follow this order manually. Do not publish a main package before the platform packages it references already exist on npm.

If `octocode-cli` changed, publish it after the MCP runtime assets are built and `packages/octocode-cli/scripts/prepack.mjs` passes.

## Runtime Packaging

`octocode-mcp` depends on three native runtime assets:

| Runtime asset | npm resolution | Approx platform size |
|---|---|---|
| Secret detection | `octocode-security-{platform}` | 1.5 MB |
| Code minification | `@octocodeai/octocode-context-utils-{platform}` | 34 MB |
| Ripgrep | `@vscode/ripgrep-{platform}` | 5 MB |

The optional-dependency model keeps the npm tarball near 562 KB and reduces a typical user install from roughly 245 MB to 40-42 MB.

Native package versions are exact-pinned in each main package's `optionalDependencies`. When releasing a native package, keep the main package version, every platform package version, and every optional dependency entry identical.

Native resolution checks:

```text
octocode-security:
1. OCTOCODE_SECURITY_NATIVE_PATH
2. platform optionalDependency package
3. dist/runtime/security
4. package root .node file
5. JS fallback

ripgrep:
1. OCTOCODE_RG_PATH
2. dist/runtime/rg
3. sibling binary next to executable
4. @vscode/ripgrep
5. system PATH
```

Standalone binaries do not use npm optional dependency resolution at runtime. The `build:bin:*` scripts build the Bun executable and populate `dist/` with the runtime assets needed by that target.

## Release Procedures

### CI

**File:** [`.github/workflows/ci.yml`](https://github.com/bgauryy/octocode-mcp/blob/main/.github/workflows/ci.yml)

Triggered by pull requests.

| Job | Purpose |
|---|---|
| `checks` | `yarn health:check`, `yarn docs:verify`, `yarn lint`, package type builds, `yarn typecheck` |
| `build-and-test` | Current-platform native build, `yarn build`, output verification, `yarn test` with coverage |
| `rust-audit` | `cargo audit` for `octocode-security` and `octocode-context-utils` |
| `pr-validation-complete` | Fails if any required CI job failed |

PR CI builds only the current runner's native `.node` files. Manual release builds must cover the full six-platform matrix.

### Manual npm Publish

Matrix build targets:

| Platform package target | Build command |
|---|---|
| `darwin-arm64` | `yarn workspace octocode-security run build:rust:darwin-arm64` and `yarn workspace @octocodeai/octocode-context-utils run build:darwin-arm64` |
| `darwin-x64` | `yarn workspace octocode-security run build:rust:darwin-x64` and `yarn workspace @octocodeai/octocode-context-utils run build:darwin-x64` |
| `linux-x64-gnu` | `yarn workspace octocode-security run build:rust:linux-x64-gnu` and `yarn workspace @octocodeai/octocode-context-utils run build:linux-x64-gnu` |
| `linux-arm64-gnu` | `yarn workspace octocode-security run build:rust:linux-arm64-gnu` and `yarn workspace @octocodeai/octocode-context-utils run build:linux-arm64-gnu` |
| `linux-x64-musl` | `yarn workspace octocode-security run build:rust:linux-x64-musl` and `yarn workspace @octocodeai/octocode-context-utils run build:linux-x64-musl` |
| `win32-x64-msvc` | `yarn workspace octocode-security run build:rust:windows-x64` and `yarn workspace @octocodeai/octocode-context-utils run build:windows-x64` |

Manual publish flow:

```bash
npm whoami

# Build every platform .node.
yarn build:native:all

# Copy every platform .node into its matching npm/{platform} directory.
cp packages/octocode-security/octocode-security.darwin-arm64.node packages/octocode-security/npm/darwin-arm64/
cp packages/octocode-security/octocode-security.darwin-x64.node packages/octocode-security/npm/darwin-x64/
cp packages/octocode-security/octocode-security.linux-x64-gnu.node packages/octocode-security/npm/linux-x64-gnu/
cp packages/octocode-security/octocode-security.linux-x64-musl.node packages/octocode-security/npm/linux-x64-musl/
cp packages/octocode-security/octocode-security.linux-arm64-gnu.node packages/octocode-security/npm/linux-arm64-gnu/
cp packages/octocode-security/octocode-security.win32-x64-msvc.node packages/octocode-security/npm/win32-x64-msvc/

cp packages/octocode-context-utils/octocode-context-utils.darwin-arm64.node packages/octocode-context-utils/npm/darwin-arm64/
cp packages/octocode-context-utils/octocode-context-utils.darwin-x64.node packages/octocode-context-utils/npm/darwin-x64/
cp packages/octocode-context-utils/octocode-context-utils.linux-x64-gnu.node packages/octocode-context-utils/npm/linux-x64-gnu/
cp packages/octocode-context-utils/octocode-context-utils.linux-x64-musl.node packages/octocode-context-utils/npm/linux-x64-musl/
cp packages/octocode-context-utils/octocode-context-utils.linux-arm64-gnu.node packages/octocode-context-utils/npm/linux-arm64-gnu/
cp packages/octocode-context-utils/octocode-context-utils.win32-x64-msvc.node packages/octocode-context-utils/npm/win32-x64-msvc/

# Dry-run each platform package before publishing.
npm publish packages/octocode-security/npm/darwin-arm64 --access public --provenance --dry-run
npm publish packages/octocode-context-utils/npm/darwin-arm64 --access public --provenance --dry-run

# Publish platform packages first, then main native packages, then octocode-mcp.
npm publish packages/octocode-security/npm/darwin-arm64 --access public --provenance
npm publish packages/octocode-security/npm/darwin-x64 --access public --provenance
npm publish packages/octocode-security/npm/linux-x64-gnu --access public --provenance
npm publish packages/octocode-security/npm/linux-x64-musl --access public --provenance
npm publish packages/octocode-security/npm/linux-arm64-gnu --access public --provenance
npm publish packages/octocode-security/npm/win32-x64-msvc --access public --provenance
npm publish packages/octocode-security --access public --provenance --ignore-scripts

npm publish packages/octocode-context-utils/npm/darwin-arm64 --access public --provenance
npm publish packages/octocode-context-utils/npm/darwin-x64 --access public --provenance
npm publish packages/octocode-context-utils/npm/linux-x64-gnu --access public --provenance
npm publish packages/octocode-context-utils/npm/linux-x64-musl --access public --provenance
npm publish packages/octocode-context-utils/npm/linux-arm64-gnu --access public --provenance
npm publish packages/octocode-context-utils/npm/win32-x64-msvc --access public --provenance
npm publish packages/octocode-context-utils --access public --provenance --ignore-scripts

OCTOCODE_RUNTIME_PLATFORMS=all yarn workspace octocode-mcp build:publish
npm publish packages/octocode-mcp --access public --provenance --ignore-scripts --dry-run
npm publish packages/octocode-mcp --access public --provenance --ignore-scripts
```

Run the `--dry-run` variant for every package when validating a release candidate. The example shows the pattern without repeating every command twice.

### Homebrew Tap

**Repo:** [`bgauryy/homebrew-octocode`](https://github.com/bgauryy/homebrew-octocode)

The Homebrew formula installs the published `octocode-cli` npm tarball and exposes the `octocode` command. The tap is downstream of npm: publish `octocode-cli` first, then bump the tap formula to that exact tarball and SHA.

Keep the tap as a separate repository. Locally, prefer a sibling checkout such as `/Users/guybary/Documents/homebrew-octocode` instead of nesting the tap inside this monorepo.

Use a single cross-platform formula for `octocode-cli`. Do not encode Octocode's internal runtime matrix in the Homebrew formula unless the tap switches to downloading standalone GitHub Release binaries. The smart split is:

| Layer | Responsibility |
|---|---|
| `octocode-cli` npm package | Bundle CLI JS, bundled skills, MCP direct-tool runtime, and copied runtime assets needed by local tools |
| `package.json` dependencies | Declare runtime Node dependencies that are intentionally externalized from the CLI bundle |
| Homebrew formula | Install the published npm tarball with npm, depend on Node, symlink `octocode`, and smoke test the command |

Dependency rules for `octocode-cli`:

- If code is bundled by esbuild, keep it in `devDependencies`.
- If code is externalized by `build.mjs`, it must be in `dependencies` so npm installs it under Homebrew.
- Workspace packages such as `octocode-mcp` and `octocode-shared` must either stay bundled or become real published runtime dependencies.
- Native or platform-sensitive assets used by CLI local tools must either be copied into `out/runtime` and verified, or resolved by real npm `optionalDependencies`. Do not rely on workspace-only dev dependencies for Homebrew.
- Runtime files copied into `out/runtime` must be real files in the packed tarball. Do not rely on symlinks; npm does not include symlinks in packages.

Use these npm package rules when deciding whether a file or dependency is available after `brew install octocode`:

| Need | npm mechanism | Octocode rule |
|---|---|---|
| `octocode` command | `package.json` `bin` | Keep `bin.octocode` pointed at `out/octocode-cli.js`; the file must start with `#!/usr/bin/env node`. |
| CLI bundle and runtime files | `package.json` `files` | Keep `out` in `files`; `out/runtime/**` and `out/runtime-assets.json` must appear in `npm pack --dry-run`. |
| Build-only tooling | `devDependencies` | Safe only when the build output is already in `out`; Homebrew users do not install dev dependencies. |
| Runtime JS loaded after install | `dependencies` | Any externalized import from `build.mjs` must be declared here. |
| Platform-specific npm packages | `optionalDependencies` | Use only when the runtime has a fallback or handles missing platform packages intentionally. |
| Single-tarball dependency vendoring | `bundleDependencies` | Avoid by default; use only if a dependency must be physically bundled into the `octocode-cli` tarball instead of resolved from npm. |

Before publishing `octocode-cli`, confirm its package output includes the runtime assets needed by local tools and that npm will install every external dependency:

```bash
OCTOCODE_RUNTIME_PLATFORMS=all yarn workspace octocode-mcp build:publish
yarn workspace octocode-cli build
cd packages/octocode-cli
node scripts/prepack.mjs
npm pack --dry-run --json
```

`prepack` must pass before `npm publish`; npm runs it for both `npm pack` and `npm publish`. It rejects a CLI package whose copied MCP runtime manifest is not all-platform. `npm pack --dry-run --json` must show `out/`, `out/runtime/`, `out/runtime-assets.json`, `skills/`, `README.md`, and `LICENSE`. This catches the common failure where `octocode --help` works but `octocode tools localSearchCode ...` fails because bundled ripgrep is missing.

Before publishing, inspect the packed file list:

```bash
npm pack --dry-run --json \
  | node -e 'let s="";process.stdin.on("data",d=>s+=d);process.stdin.on("end",()=>{const files=JSON.parse(s)[0].files.map(f=>f.path); for (const must of ["out/octocode-cli.js","out/runtime-assets.json","skills/README.md","README.md","LICENSE"]) { if (!files.includes(must)) throw new Error(`missing ${must}`); } if (!files.some(f=>f.startsWith("out/runtime/rg/"))) throw new Error("missing bundled rg runtime"); console.log("pack file list ok");})'
```

The tap formula should stay small and platform-neutral:

```ruby
require "language/node"

class Octocode < Formula
  desc "Code research CLI for Octocode MCP"
  homepage "https://octocode.ai"
  url "https://registry.npmjs.org/octocode-cli/-/octocode-cli-X.Y.Z.tgz"
  sha256 "..."
  license "MIT"

  depends_on "node"

  def install
    system "npm", "install", *std_npm_args
    bin.install_symlink Dir["#{libexec}/bin/*"]
  end

  test do
    assert_match "octocode v#{version}", shell_output("#{bin}/octocode --version")
    assert_match "localSearchCode", shell_output("#{bin}/octocode tools")

    (testpath/"fixture.txt").write("octocode homebrew smoke\n")
    output = shell_output("#{bin}/octocode tools localSearchCode --queries " \
                          "'{\"path\":\"#{testpath}\",\"keywords\":\"octocode\",\"maxFiles\":1}'")
    assert_match "fixture.txt", output
  end
end
```

Use Homebrew `on_macos`, `on_linux`, `on_arm`, and `on_intel` blocks only if the formula installs per-platform standalone release archives instead of the npm tarball. Do not mix that model with `octocode-cli`; npm and the CLI package should own dependency resolution for the npm formula.

After `octocode-cli` is published, update the tap:

```bash
cd /Users/guybary/Documents/homebrew-octocode
./scripts/update-formula.sh X.Y.Z
brew style Formula/octocode.rb
brew audit --strict --online Formula/octocode.rb
```

Smoke test the tap install:

```bash
brew uninstall octocode || true
brew install bgauryy/octocode/octocode
octocode --version
octocode tools
octocode tools localSearchCode --queries '{"path":".","keywords":"octocode","maxFiles":1}'
```

If the CLI tool surface changes, update the tap README in the same tap PR. Keep the tool count and LSP tool name aligned with the current CLI help; the current surface is 12 tools with `lspGetSemanticContent` as the unified LSP tool.

### Manual Binary Releases

Build the six standalone binaries manually, calculate checksums, and attach the artifacts to the GitHub Release for tag `vX.Y.Z`.

```bash
yarn workspace octocode-mcp run build:bin:darwin-arm64
yarn workspace octocode-mcp run build:bin:darwin-x64
yarn workspace octocode-mcp run build:bin:linux-arm64
yarn workspace octocode-mcp run build:bin:linux-x64
yarn workspace octocode-mcp run build:bin:linux-x64-musl
yarn workspace octocode-mcp run build:bin:windows-x64

cd packages/octocode-mcp/dist
shasum -a 256 octocode-mcp-* > checksums-sha256.txt
```

Upload the six binaries plus `checksums-sha256.txt` to the GitHub Release. Smoke test at least one downloaded artifact before announcing the release.

## Native Build Commands

Current platform:

```bash
yarn workspace octocode-security run build
yarn workspace @octocodeai/octocode-context-utils run build
```

Specific release target:

```bash
yarn workspace octocode-security run build:rust:darwin-arm64
yarn workspace octocode-security run build:rust:linux-x64-gnu

yarn workspace @octocodeai/octocode-context-utils run build:darwin-arm64
yarn workspace @octocodeai/octocode-context-utils run build:linux-x64-gnu
```

## Local Development Build

Fast MCP build:

```bash
cd packages/octocode-mcp
yarn build
```

Full current-platform build:

```bash
yarn workspace octocode-security run build
yarn workspace @octocodeai/octocode-context-utils run build
cd packages/octocode-mcp
yarn build
```

Yarn workspace links let local builds load native `.node` files from the package roots after the native packages are built.

## References

- [npm package.json: `files`, `bin`, `dependencies`, `bundleDependencies`, `optionalDependencies`](https://docs.npmjs.com/cli/v11/configuring-npm/package-json/)
- [npm publish: files included in package and `npm pack --dry-run`](https://docs.npmjs.com/cli/v11/commands/npm-publish/)
- [npm pack: dry-run and JSON output](https://docs.npmjs.com/cli/v11/commands/npm-pack/)
- [npm scripts: `prepack` / `prepare` / `postpack` lifecycle order](https://docs.npmjs.com/cli/v11/using-npm/scripts/)
- [Homebrew Formula Cookbook: `std_npm_args`](https://docs.brew.sh/Formula-Cookbook#std_npm_args)
- [Homebrew Formula Cookbook: platform blocks](https://docs.brew.sh/Formula-Cookbook#handling-different-system-configurations)
