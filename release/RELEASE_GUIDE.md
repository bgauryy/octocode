# Release Guide

> Build, publish, and ship all Octocode packages — npm, native binaries, standalones, and Homebrew.

## Contents

- [Package Dependency Flows](#package-dependency-flows)
- [Native Binaries](#native-binaries)
- [Dev Workflow](#dev-workflow)
- [Build](#build)
- [Publish](#publish)
- [Standalone Binaries](#standalone-binaries)
- [Homebrew](#homebrew)

---

## Package Dependency Flows

Two entry points. Each ships to users independently:

```
npx octocode-mcp          →  octocode-mcp
npx octocode / Homebrew   →  octocode-cli
```

### Dependency tree

```
octocode-mcp ──────────────────────────────────────────────────────────────────┐
octocode-cli ──────────────────────────────────────────────────────────────────┤
    │                                                                           │
    └─▶ @octocodeai/octocode-tools-core   (compiled; bundles octokit/node-cache)
              ├─▶ octocode-security          (Rust .node — secret detection)
              ├─▶ @octocodeai/octocode-context-utils  (Rust .node — minify/YAML)
              ├─▶ octocode-lsp               (Rust .node — LSP engine; TS wrapper)
              └─▶ octocode-shared            (credentials/session/platform)
```

`octocode-mcp` adds `@modelcontextprotocol/sdk` and the MCP server layer on top. `octocode-cli` skips the MCP layer and talks to `octocode-tools-core` directly — so the CLI has no runtime dependency on `octocode-mcp`.

### What each package bundles vs. externalizes

| Package | Bundles (esbuild packs into dist) | Externalizes (npm installs separately) |
|---|---|---|
| `octocode-security` | — | — (no runtime JS deps) |
| `@octocodeai/octocode-context-utils` | — | — (no runtime JS deps) |
| `octocode-shared` | — | `@octokit/oauth-methods`, `@octokit/request`, `zod` |
| `octocode-lsp` | — | `vscode-*`, `zod`, `octocode-security`, `octocode-shared`; **ships its own Rust `.node` via 6 platform `optionalDependencies`** |
| `@octocodeai/octocode-tools-core` | `@octokit/*`, `octokit`, `node-cache`, `zod` | `octocode-{security,lsp,shared}`, `@octocodeai/{octocode-context-utils,octocode-core}`, `@modelcontextprotocol/sdk`, `@vscode/ripgrep`, `typescript`, `typescript-language-server` |
| `octocode-mcp` | `zod` | `@modelcontextprotocol/sdk`, `@octocodeai/{octocode-tools-core,octocode-core}`, `octocode-{security,shared}` |
| `octocode-cli` | `@inquirer/*`, `@octokit/*`, `open`, `zod` | `@octocodeai/octocode-tools-core`, `octocode-shared` |

---

## Native Binaries

**Three** packages compile Rust to `.node` native addons via [napi-rs](https://napi.rs/docs/deep-dive/release): `octocode-security`, `@octocodeai/octocode-context-utils`, and `octocode-lsp`. Each owns its own binary distribution — no other package copies or re-declares the binaries.

All three native packages now have matching tooling: `napi` config + 6 platform `optionalDependencies`, per-target cross-compile scripts (`build:<target>`), a `build:all` aggregate, a `pack:check` guard for the root loader, a `platforms:check` guard for the six platform tarballs, and inclusion in the root `build:native:all` script. `octocode-lsp` also ships a `bundle-lsp.mjs` so the standalone binary contains the lsp `.node`.

### How they ship (napi-rs pattern)

Each native package publishes one root package + 6 platform-specific packages:

```
octocode-security                        ← JS loader only (no .node in tarball)
  optionalDependencies:
    octocode-security-darwin-arm64       ← contains .node for macOS Apple Silicon
    octocode-security-darwin-x64
    octocode-security-linux-arm64-gnu
    octocode-security-linux-x64-gnu
    octocode-security-linux-x64-musl     ← musl declares libc: ["musl"]
    octocode-security-win32-x64-msvc
```

At `npm install` time, npm checks `os` + `cpu` + `libc` on each platform package and installs only the match. The user gets exactly one `.node` file per native package.

### How users get the binaries

Both entry points deliver the same binaries through `octocode-tools-core`:

```
npm install octocode-mcp            npm install octocode-cli
  └─ @octocodeai/octocode-tools-core  └─ @octocodeai/octocode-tools-core
       └─ octocode-security                  └─ octocode-security
       └─ @octocodeai/octocode-               └─ @octocodeai/octocode-
            context-utils                          context-utils
```

### Runtime loader resolution order

The JS loaders in each native package try paths in this order:

1. `OCTOCODE_SECURITY_NATIVE_PATH` / `OCTOCODE_CONTEXT_NATIVE_PATH` env override
2. `.node` next to the loader ← dev build only
3. `dist/runtime/{security,context-utils}/` ← standalone binary layout
4. `../runtime/{security,context-utils}/` ← CLI standalone layout
5. platform `optionalDependency` ← **standard npm install path**
6. napi-rs CJS fallback loader

### Local development (Yarn workspaces)

The `npm/{platform}/` directories are declared as workspaces in the root `package.json`. Yarn resolves the exact-pinned `optionalDependencies` versions to the local workspace — no root-level `resolutions` needed.

---

## Dev Workflow

Day-to-day development loop for working across the Rust/TS library packages and the interface packages that consume them.

### First-time setup

```bash
yarn install
```

This wires all workspace packages together. Because every internal dep uses `workspace:*`, Yarn links siblings directly — no npm publish needed to test changes locally.

If any `package.json` has a pinned version instead of `workspace:*` (e.g. after a publish run), restore workspace refs:

```bash
node release/sync-packages-local.mjs --fix
yarn install
```

### Build native Rust libs for your platform

`octocode-security` and `octocode-context-utils` compile to `.node` native addons. Build only the platform you're on:

```bash
# macOS Apple Silicon
yarn workspace octocode-security run build:rust:darwin-arm64
yarn workspace @octocodeai/octocode-context-utils run build:darwin-arm64

# macOS Intel
yarn workspace octocode-security run build:rust:darwin-x64
yarn workspace @octocodeai/octocode-context-utils run build:darwin-x64
```

The compiled `.node` is placed next to the JS loader. In dev the loader resolves it via the "`.node` next to the loader" path — no install step needed.

### Build TS packages

```bash
# From the package directory you changed:
yarn build:dev    # bundle only, skip lint (fast)
yarn build        # lint + bundle

# Or rebuild everything from the repo root:
yarn workspaces foreach -pt run build:dev
```

Build order matters. Always build dependencies before consumers:

```
octocode-shared → octocode-security → octocode-context-utils → octocode-lsp
  → octocode-tools-core → octocode-mcp / octocode-cli
```

### Test

```bash
# From any package directory:
yarn test          # run with coverage
yarn test:watch    # watch mode
yarn test:quiet    # minimal output

# From the root (all packages):
yarn verify        # lint + typecheck + tests everywhere
```

### Keep versions in sync

When you bump the version in `packages/octocode-mcp/package.json`, align every other package:

```bash
node release/sync-packages-version.mjs
# → sets the same version on every package.json
# → converts any pinned internal dep back to workspace:*
```

To check that no package has accidentally lost its `workspace:*` ref:

```bash
node release/sync-packages-local.mjs          # exits 1 on any violation
node release/sync-packages-local.mjs --verbose # see every dep that was checked
```

### Typical change cycle

```
1. Edit source in packages/octocode-security|lsp|context-utils|tools-core
2. yarn build:dev  (from the changed package)
3. yarn build:dev  (from octocode-tools-core if a lib changed)
4. yarn test       (from the affected package)
5. Test end-to-end from octocode-mcp or octocode-cli
```

---

## Build

### Prerequisites

```bash
yarn install
```

### Build a package

```bash
# From the package directory:
yarn build        # lint + bundle
yarn build:dev    # bundle only (skip lint)
```

### Build native binaries (Rust)

```bash
# Build all 6 platforms for all three native packages
# (security + context-utils + lsp), then verify every platform .node exists:
yarn build:native:all
yarn platforms:check          # fails if any platform dir is missing its .node

# Or one platform at a time:
yarn workspace octocode-security run build:rust:darwin-arm64
yarn workspace @octocodeai/octocode-context-utils run build:darwin-arm64
yarn workspace octocode-lsp run build:darwin-arm64
```

The build scripts copy the compiled `.node` into `npm/{platform}/` automatically.

> ⚠️ A plain `yarn workspace <pkg> run build` (no target) compiles **only the host
> platform** and leaves the other five `npm/{platform}/` dirs empty. Always use
> `build:all` (or per-target builds on matching CI runners) before publishing, and
> let `platforms:check` confirm all six are present.

---

## Publish

### How users get binaries after `npm install`

When a user runs `npm install octocode-mcp` or `npm install octocode-cli`, npm resolves the full dependency tree automatically:

```
npm install octocode-mcp  (or octocode-cli)
  └─ @octocodeai/octocode-tools-core
       ├─ octocode-security
       │    └─ optionalDependencies:
       │         octocode-security-darwin-arm64   ← installed on macOS Apple Silicon
       │         octocode-security-darwin-x64     ← installed on macOS Intel
       │         octocode-security-linux-x64-gnu  ← installed on Linux x64
       │         octocode-security-linux-x64-musl ← installed on Alpine/musl
       │         octocode-security-linux-arm64-gnu
       │         octocode-security-win32-x64-msvc
       └─ @octocodeai/octocode-context-utils
            └─ optionalDependencies: (same 6 platforms as above)
```

npm uses `os`, `cpu`, and `libc` fields on each platform package to install **exactly one `.node` file** — the one that matches the user's machine. No post-install scripts, no compilation on the user's machine.

**This only works if all packages are published to npm.** Publishing in the correct order (dependencies before consumers) and publishing every platform sub-package before the root loader is what makes `npm install` deliver the right binary to end users.

### Pre-publish checks

```bash
# 1. Bump the version in packages/octocode-mcp/package.json, then:
#    Sync version to every package AND pin all internal deps to exact version.
#    This also rewrites the external file: dep on @octocodeai/octocode-core
#    (sibling repo) to its published semver — see EXTERNAL_FILE_DEPS in the script.
#    (neither workspace: nor file: refs are valid on npm — this step removes both):
node release/sync-packages-version.mjs --pin-for-publish

# 2. Verify no workspace: or file: refs remain — npm publish will fail if any do:
rg '"workspace:' packages/*/package.json packages/*/npm/*/package.json
rg '"file:'      packages/*/package.json packages/*/npm/*/package.json
# → both must be empty

# Root native packages must NOT contain a .node (all three have a pack:check guard):
yarn workspace octocode-security run pack:check
yarn workspace @octocodeai/octocode-context-utils run pack:check
yarn workspace octocode-lsp run pack:check

# Each platform package must contain exactly one non-empty .node.
# This is the critical guard: a host-only build populates just ONE of the six
# platform dirs and silently skips the rest. Run from the repo root — it checks
# all three native packages and exits 1 if ANY platform binary is missing/empty:
yarn platforms:check
# (or per-package: yarn workspace octocode-lsp run platforms:check)
#
# Belt-and-suspenders: each of the 18 platform packages also has a prepublishOnly
# hook (node ../verify-binary.cjs) that aborts `npm publish` for that package if
# its own .node is missing — so even a forgotten platforms:check cannot ship an
# empty platform tarball.

# All tests pass:
yarn verify
```

### Publish order

Dependencies must exist on npm before dependents. Publish in this order:

```
1. octocode-shared
2. octocode-security npm/{platform} × 6
3. octocode-security
4. @octocodeai/octocode-context-utils npm/{platform} × 6
5. @octocodeai/octocode-context-utils
6. octocode-lsp npm/{platform} × 6
7. octocode-lsp
8. @octocodeai/octocode-tools-core
9. octocode-mcp
10. octocode-cli  →  then update Homebrew tap
```

### Publish commands

```bash
npm whoami   # confirm auth

# Dry-run first:
npm publish packages/octocode-shared            --access public --provenance --dry-run
for dir in packages/octocode-security/npm/*; do
  npm publish "$dir" --access public --provenance --dry-run
done
# ... repeat for context-utils and octocode-lsp, then:
npm publish packages/octocode-tools-core        --access public --provenance --dry-run
npm publish packages/octocode-mcp               --access public --provenance --ignore-scripts --dry-run
npm publish packages/octocode-cli               --access public --provenance --dry-run

# Live publish (same commands, drop --dry-run):
npm publish packages/octocode-shared            --access public --provenance

npm publish packages/octocode-security/npm/darwin-arm64   --access public --provenance
npm publish packages/octocode-security/npm/darwin-x64     --access public --provenance
npm publish packages/octocode-security/npm/linux-x64-gnu  --access public --provenance
npm publish packages/octocode-security/npm/linux-x64-musl --access public --provenance
npm publish packages/octocode-security/npm/linux-arm64-gnu --access public --provenance
npm publish packages/octocode-security/npm/win32-x64-msvc --access public --provenance
npm publish packages/octocode-security          --access public --provenance --ignore-scripts

npm publish packages/octocode-context-utils/npm/darwin-arm64   --access public --provenance
npm publish packages/octocode-context-utils/npm/darwin-x64     --access public --provenance
npm publish packages/octocode-context-utils/npm/linux-x64-gnu  --access public --provenance
npm publish packages/octocode-context-utils/npm/linux-x64-musl --access public --provenance
npm publish packages/octocode-context-utils/npm/linux-arm64-gnu --access public --provenance
npm publish packages/octocode-context-utils/npm/win32-x64-msvc --access public --provenance
npm publish packages/octocode-context-utils     --access public --provenance --ignore-scripts

npm publish packages/octocode-lsp/npm/darwin-arm64    --access public --provenance
npm publish packages/octocode-lsp/npm/darwin-x64      --access public --provenance
npm publish packages/octocode-lsp/npm/linux-x64-gnu   --access public --provenance
npm publish packages/octocode-lsp/npm/linux-x64-musl  --access public --provenance
npm publish packages/octocode-lsp/npm/linux-arm64-gnu --access public --provenance
npm publish packages/octocode-lsp/npm/win32-x64-msvc  --access public --provenance
npm publish packages/octocode-lsp               --access public --provenance

npm publish packages/octocode-tools-core        --access public --provenance
npm publish packages/octocode-mcp               --access public --provenance --ignore-scripts
npm publish packages/octocode-cli               --access public --provenance
```

### Restore workspace refs after publish

After publishing, internal deps are pinned to the exact version. Restore `workspace:*` so local development works again:

```bash
node release/sync-packages-version.mjs   # converts pinned refs back to workspace:*
yarn install                             # re-sync the lockfile
```

Or verify first, then fix:

```bash
node release/sync-packages-local.mjs        # shows any remaining pinned refs
node release/sync-packages-local.mjs --fix  # rewrites them to workspace:*
yarn install
```

### Smoke test after publish

```bash
tmp=$(mktemp -d) && cd "$tmp" && npm init -y >/dev/null
npm install octocode-mcp@X.Y.Z octocode-cli@X.Y.Z
node --input-type=module -e "const s = await import('octocode-security'); console.log('security:', Boolean(s.securityRegistry))"
node --input-type=module -e "const c = await import('@octocodeai/octocode-context-utils'); console.log('context-utils:', c.getSupportedSignatureExtensions().length > 0)"
node --input-type=module -e "await import('octocode-lsp'); console.log('lsp: native .node loaded ✓')"  # throws if no platform optionalDependency was installed
npx octocode-mcp --help
npx octocode --version
```

---

## Standalone Binaries

Standalone binaries bundle Bun + all runtime assets (`rg`, `.node` files) into a single executable per platform. Used for the GitHub Release and Homebrew tap.

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

Upload the 6 binaries + `checksums-sha256.txt` to the GitHub Release for `vX.Y.Z`.

**Layout inside each standalone binary** (built by `bundle-rg.mjs`, `bundle-security.mjs`, `bundle-context-utils.mjs`):

```
dist/
  octocode-mcp-darwin-arm64
  runtime/
    rg/rg-darwin-arm64
    security/octocode-security.darwin-arm64.node
    context-utils/octocode-context-utils.darwin-arm64.node
```

The loaders check `dist/runtime/` before falling back to npm `optionalDependencies`, so standalone users need no npm install.

---

## Homebrew

**Tap repo:** [`bgauryy/homebrew-octocode`](https://github.com/bgauryy/homebrew-octocode)

The formula installs the published `octocode-cli` npm tarball. Publish `octocode-cli` first, then:

```bash
cd /Users/guybary/Documents/homebrew-octocode
./scripts/update-formula.sh X.Y.Z
brew style Formula/octocode.rb
brew audit --strict --online Formula/octocode.rb
brew uninstall octocode || true
brew install bgauryy/octocode/octocode
octocode --version && octocode tools
```

Homebrew users get native binaries through the same `optionalDependencies` chain: `octocode-cli → octocode-tools-core → octocode-security + octocode-context-utils → platform .node`.

---

## References

- [napi-rs release docs](https://napi.rs/docs/deep-dive/release)
- [npm `os`/`cpu`/`libc` selectors](https://docs.npmjs.com/cli/v11/commands/npm-install/)
- [npm `optionalDependencies`](https://docs.npmjs.com/cli/v11/configuring-npm/package-json/)
