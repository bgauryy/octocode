# Release Guide

> Build, publish, and ship the Octocode packages — npm, native binaries, standalones, and Homebrew.

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

Two interface packages ship to users independently:

```
npx octocode-mcp          →  octocode-mcp
npx octocode / Homebrew   →  octocode
```

Only these workspace packages are part of the monorepo:

| Directory | npm package | Role |
|---|---|---|
| `packages/octocode` | `octocode` | CLI/interface package: direct tool runner, auth, install, status, token, MCP marketplace, cache, and skills workflows. |
| `packages/octocode-mcp` | `octocode-mcp` | MCP server/interface package for AI assistants. |
| `packages/octocode-engine` | `@octocodeai/octocode-engine` | Rust-based native engine. |
| `packages/octocode-tools-core` | `@octocodeai/octocode-tools-core` | Core tool implementations and shared credentials/session/config/platform utilities. **Bundled into its consumers at build time — NOT published to npm** (see [Bundled, not published](#tools-core-is-bundled-not-published)). |
| `packages/octocode-vscode` | `octocode-mcp-vscode` | VS Code extension. |

### Dependency tree

`@octocodeai/octocode-tools-core` is **inlined into each interface package's build output** by esbuild — it is a build-time (dev) dependency, never an installed runtime dependency. tools-core's own runtime deps cannot be inlined (the native engine especially), so they are hoisted into each consumer's `dependencies` and installed directly from npm:

```
octocode-mcp ─┐  (tools-core inlined into dist/index.js)
octocode ─────┤  (tools-core inlined into out/octocode.js)
              │
              ├─▶ @octocodeai/octocode-engine  (Rust .node native engine — unbundlable)
              ├─▶ @octocodeai/octocode-core     (schemas / types)
              ├─▶ octokit, @octokit/*, node-cache, zod
              └─▶ @modelcontextprotocol/sdk     (octocode-mcp only)
```

`octocode-mcp` adds `@modelcontextprotocol/sdk` and the MCP server layer on top. `octocode` skips the MCP layer entirely (it never references the SDK). Neither CLI has a runtime dependency on the other, and neither installs `@octocodeai/octocode-tools-core` — its code rides along inside their bundles.

### Publish/install invariant

There are two graphs to keep separate:

- **Source/build graph**: `@octocodeai/octocode-engine` →
  `@octocodeai/octocode-tools-core` → `octocode-mcp` / `octocode`.
- **npm install graph**: `octocode-mcp` / `octocode` →
  `@octocodeai/octocode-engine` → one matching platform
  `@octocodeai/octocode-engine-*` optional dependency.

`@octocodeai/octocode-tools-core` is intentionally missing from the npm install
graph. Therefore the engine root and all six engine platform packages must exist
on npm before publishing any interface package that depends on the engine.

### What each package bundles vs. externalizes

| Package | Bundles (build output packs into dist/out) | Externalizes (npm installs separately) |
|---|---|---|
| `@octocodeai/octocode-engine` | TypeScript wrapper/build output | `zod`; ships its Rust `.node` via 6 platform `optionalDependencies` |
| `@octocodeai/octocode-tools-core` | Core tool implementation build output (consumed locally only — **not published**) | `@modelcontextprotocol/sdk`, `@octocodeai/octocode-core`, `@octocodeai/octocode-engine`, `@octokit/*`, `node-cache`, `octokit`, `zod` |
| `octocode-mcp` | MCP server build output **+ inlined `@octocodeai/octocode-tools-core`** | `@modelcontextprotocol/sdk`, `@octocodeai/octocode-core`, `@octocodeai/octocode-engine`, `@octokit/oauth-methods`, `@octokit/plugin-throttling`, `@octokit/request`, `node-cache`, `octokit`, `zod` |
| `octocode` | CLI build output **+ inlined `@octocodeai/octocode-tools-core`** **+ the bundled `skills/` dir** (copied from repo root by `prepack.mjs`, `.env.example` files stripped) | `@inquirer/prompts`, `@octocodeai/octocode-core`, `@octocodeai/octocode-engine`, `@octokit/auth-oauth-device`, `@octokit/oauth-methods`, `@octokit/plugin-throttling`, `@octokit/request`, `node-cache`, `octokit`, `open`, `zod` |
| `octocode-mcp-vscode` | VS Code extension bundle | VS Code runtime APIs |

> The `octocode` tarball ships the repo-root `skills/` directory verbatim (every
> skill under `skills/`, e.g. `octocode`, `octocode-research`, `octocode-rfc-generator`),
> bundled by `packages/octocode/scripts/prepack.mjs`. Before publishing `octocode`,
> make sure those skills are current — stale CLI examples, tool names, or modes in a
> skill ship to every user. `docs/` does **not** ship in the tarball (`files` is
> `out`, `skills`, `assets/example.png`, `README.md`, `LICENSE`).

### tools-core is bundled, not published

`@octocodeai/octocode-tools-core` holds the shared tool implementations, but it is **never published to npm**. Each interface package inlines it at build time and ships it inside its own tarball. This removes one published package from the release and guarantees the consumers always run the exact tools-core build they were compiled against.

How it works:

1. **Runtime JS** — esbuild inlines tools-core because it is *not* listed in the bundler `external` set (`packages/octocode-mcp/buildConfig.mjs`, `packages/octocode/build.mjs`). tools-core's own runtime deps stay external (the native engine cannot be bundled; the rest are normal registry packages) and are declared in each consumer's `dependencies` so `npm install` pulls them directly — no tools-core hop.
2. **Manifest** — `@octocodeai/octocode-tools-core` lives in each consumer's **`devDependencies`** as `workspace:^` (a build-time-only link). It is absent from `dependencies`, so consumers never try to fetch it. `npm publish` auto-corrects the leftover `workspace:` devDep ref; `sync-packages-version.mjs` keeps it on the workspace protocol even in `--pin-for-publish` mode (it is in `UNPUBLISHED_INTERNAL`), so it is never pinned to a non-existent registry version.
3. **Types (octocode-mcp only)** — `tsc` emits per-file `.d.ts` into `dist/.types`, then `scripts/bundle-dts.mjs` (rollup + `rollup-plugin-dts`) rolls `public.d.ts` into a single declaration file that **inlines tools-core's types** while keeping still-published packages (`@octocodeai/octocode-core`, `zod`) as external imports. Only the bundled `dist/public.d.ts` ships. `octocode` is `bin`-only and ships no `.d.ts`, so it needs no type bundling.

Verify after a build that no tools-core reference leaked into a published artifact:

```bash
# JS bundles + type surface must contain zero tools-core import specifiers:
grep -rl 'octocode-tools-core' packages/octocode-mcp/dist packages/octocode/out || echo "✓ clean"
# Published runtime deps must NOT include tools-core (devDeps may keep the workspace ref):
node -e "const p=require('./packages/octocode-mcp/package.json'); if(p.dependencies['@octocodeai/octocode-tools-core'])throw Error('tools-core leaked into runtime deps'); console.log('✓ not a runtime dep')"
```

---

## Native Binaries

One workspace package compiles Rust to a `.node` native addon via [napi-rs](https://napi.rs/docs/deep-dive/release): `@octocodeai/octocode-engine`.

The engine owns the Rust-backed runtime work: security scanning/sanitization, path and command validation helpers, context minification, signature extraction, structural search helpers, ripgrep parsing, diff filtering, YAML serialization, pagination offsets, and LSP support.

### How it ships (napi-rs pattern)

The native package publishes one root package + 6 platform-specific packages:

```
@octocodeai/octocode-engine                 ← JS/TS loader only (no .node in tarball)
  optionalDependencies:
    @octocodeai/octocode-engine-darwin-arm64       ← contains .node for macOS Apple Silicon
    @octocodeai/octocode-engine-darwin-x64
    @octocodeai/octocode-engine-linux-arm64-gnu
    @octocodeai/octocode-engine-linux-x64-gnu
    @octocodeai/octocode-engine-linux-x64-musl     ← musl declares libc: ["musl"]
    @octocodeai/octocode-engine-win32-x64-msvc
```

At `npm install` time, npm checks `os` + `cpu` + `libc` on each platform package and installs only the match. The user gets exactly one `.node` file for the engine package.

### How users get the binary

Both interface packages declare `@octocodeai/octocode-engine` as a direct dependency (tools-core, which used to be the intermediary, is now inlined into their bundles — see [Bundled, not published](#tools-core-is-bundled-not-published)):

```
npm install octocode-mcp            npm install octocode
  └─ @octocodeai/octocode-engine      └─ @octocodeai/octocode-engine
       └─ one matching platform           └─ one matching platform
          optionalDependency                 optionalDependency
```

### Runtime loader resolution order

The engine JS loader resolves the native binary through the package's runtime loader and platform `optionalDependencies`. Standalone binaries also place the engine `.node` under their bundled `dist/runtime/engine/` layout.

### Local development (Yarn workspaces)

The `packages/octocode-engine/npm/{platform}/` directories are declared as workspaces in the root `package.json`. Yarn resolves the exact-pinned `optionalDependencies` versions to the local workspace — no root-level `resolutions` needed.

---

## Dev Workflow

Day-to-day development loop for working across the Rust engine, core tool implementations, and interface packages that consume them.

### First-time setup

```bash
yarn install
```

This wires all workspace packages together. Because internal dependencies use `workspace:*`, Yarn links siblings directly — no npm publish needed to test changes locally.

If any `package.json` has a pinned version instead of `workspace:*` after a publish run, restore workspace refs:

```bash
node release/sync-packages-local.mjs --fix
yarn install
```

### Build

The canonical build procedure — native engine for all 6 platforms, the four-stage build chain, TS packages, and build order — lives in [Build](#build). For the dev loop:

```bash
yarn workspace @octocodeai/octocode-engine run build:darwin-arm64   # host-platform .node for local dev
yarn build:dev                                                      # from the changed TS package (fast)
```

### Test

```bash
# From any package directory:
yarn test          # run package tests when available

# From the root (all packages):
yarn verify        # lint + typecheck + tests everywhere
```

### Keep versions in sync

Versions are independent by package family: engine `16.5.x`, MCP `16.2.x`, CLI
`2.x`. Use the narrowest sync command for the release you are doing:

```bash
# Engine-only release:
yarn workspace @octocodeai/octocode-engine run version:sync

# Full same-version monorepo release only:
node release/sync-packages-version.mjs
```

Do not run `release/sync-packages-version.mjs` for an engine-only release; it
uses `packages/octocode-mcp/package.json` as its source and would collapse the
engine version onto the MCP version.

To check that no package has accidentally lost its `workspace:*` ref:

```bash
node release/sync-packages-local.mjs
node release/sync-packages-local.mjs --verbose
```

### Typical change cycle

```
1. Edit source in packages/octocode-engine or packages/octocode-tools-core
2. yarn build:dev  (from the changed package)
3. yarn build:dev  (from octocode-tools-core if the engine changed)
4. yarn test       (from the affected package)
5. Test end-to-end from octocode-mcp or octocode
```

---

## Build

### Prerequisites

```bash
yarn install          # wires workspace:* links; no npm publish needed for local builds
```

### Build the native engine — all 6 platforms (release)

The engine is the only Rust crate; it ships one `.node` per platform via 6 `optionalDependencies`. Every per-platform build runs the same four-stage chain (defined in `packages/octocode-engine/package.json`):

```
prebuild.cjs  →  napi build --platform --release [--cross-compile] --target <triple>  →  postbuild.cjs  →  tsc
```

| Stage | What it does |
|---|---|
| `prebuild.cjs` | regenerates `src/security/patterns.rs` from the canonical TS regex source via `scripts/gen-patterns.mjs` — keeps the Rust detector in lockstep with the JS fallback |
| `napi build` | compiles Rust → one `octocode-engine.<triple>.node` at the package root. `--cross-compile` is set automatically on every non-host target (napi-rs provides the zig-backed cross toolchain for Linux gnu/musl/arm64 and Windows MSVC) |
| `postbuild.cjs` | restores the canonical hand-authored loaders from `loader/` over napi's generated `index.{js,cjs,d.ts}` — the package is `"type":"module"`, and the napi-generated CJS `index.js` would break every ESM `import` (see the 16.5.0 bug below). Also copies the just-built `.node` into `npm/<platform>/` |
| `tsc` (`build:ts`) | emits `dist/` — the TS wrappers (`security/`, `lsp/`) consumed by `@octocodeai/octocode-tools-core` |

Build all six platforms on one machine, then confirm every `.node` landed in its `npm/<platform>/` dir:

```bash
yarn workspace @octocodeai/octocode-engine run build:all        # 6 native targets, then build:ts
yarn workspace @octocodeai/octocode-engine run platforms:check  # asserts all 6 .node present
```

Root shortcuts exist for both: `yarn build:native:all` / `yarn platforms:check`.

> There is **no CI matrix** building the per-platform binaries — `build:all` (cross-compile via napi-rs/zig) on a single release machine is the supported path. If a cross-target fails on your host, build that target natively on a matching runner and drop the `.node` into `npm/<platform>/`; `platforms:check` will confirm.

Per-platform scripts (one target each, release):

```bash
yarn workspace @octocodeai/octocode-engine run build:darwin-arm64     # aarch64-apple-darwin
yarn workspace @octocodeai/octocode-engine run build:darwin-x64       # x86_64-apple-darwin
yarn workspace @octocodeai/octocode-engine run build:linux-x64-gnu    # x86_64-unknown-linux-gnu   (--cross-compile)
yarn workspace @octocodeai/octocode-engine run build:linux-x64-musl   # x86_64-unknown-linux-musl  (--cross-compile)
yarn workspace @octocodeai/octocode-engine run build:linux-arm64-gnu  # aarch64-unknown-linux-gnu  (--cross-compile)
yarn workspace @octocodeai/octocode-engine run build:windows-x64      # x86_64-pc-windows-msvc     (--cross-compile)
```

> ⚠️ A plain `yarn workspace @octocodeai/octocode-engine run build` (no target) compiles **only the host platform** — fine for local dev, but it leaves the other five `npm/<platform>/` dirs empty. Never publish from a `yarn build`; publish from `build:all` + `platforms:check`.

For local dev (host platform, no release LTO): `yarn workspace @octocodeai/octocode-engine run build:dev`.

### Build TS packages

```bash
# From the changed package:
yarn build:dev      # fast dev build
yarn build          # full build
# Or everything, dependency-ordered:
yarn workspaces foreach -pt run build:dev
```

Local source build order is fixed — build dependencies before consumers:

```
@octocodeai/octocode-engine → @octocodeai/octocode-tools-core → octocode-mcp / octocode / octocode-mcp-vscode
```

This is not the npm publish graph. `@octocodeai/octocode-tools-core` is built so
the interface bundles can inline it, but it is not published.

---

## Publish

### Pre-publish checks

> **Versioned independently** (engine `16.5.x`, octocode-mcp `16.2.x`, octocode `2.x`). For an engine-only release, bump `packages/octocode-engine/package.json` and use the engine's `version:sync`. Do **not** run `release/sync-packages-version.mjs` — it forces every package to octocode-mcp's version and would downgrade the engine. `@octocodeai/octocode-tools-core` is **not published** (it is bundled into the interface packages — see [Bundled, not published](#tools-core-is-bundled-not-published)), so its own version only matters for local builds.

#### The engine is Rust → TS — verify both layers

| Layer | Produced by | Output | Guarded by |
|---|---|---|---|
| Rust native | `napi build` (×6 targets) | one `.node` per `npm/<platform>/` | `platforms:check` |
| JS/TS loader | `postbuild.cjs` restores `loader/{index.js,index.cjs,index.d.ts}` over the napi-generated files | root `index.*` (ESM `import` → `index.js`, CJS `require` → `index.cjs`) | `loader:check` |
| TS build | `tsc` | `dist/` (security + lsp) | `tsc` / `verify` |

The 6 `.npm` platform packages (`@octocodeai/octocode-engine-<platform>`) each carry exactly one `.node` and are exact-pinned in the root's `optionalDependencies`. The root tarball ships **no** `.node`.

Before publishing `octocode-mcp` or `octocode`, verify the engine package family
is already visible from the registry:

```bash
ENGINE_VERSION=$(node -p "require('./packages/octocode-engine/package.json').version")
for pkg in \
  @octocodeai/octocode-engine \
  @octocodeai/octocode-engine-darwin-arm64 \
  @octocodeai/octocode-engine-darwin-x64 \
  @octocodeai/octocode-engine-linux-arm64-gnu \
  @octocodeai/octocode-engine-linux-x64-gnu \
  @octocodeai/octocode-engine-linux-x64-musl \
  @octocodeai/octocode-engine-win32-x64-msvc
do
  npm view "$pkg@$ENGINE_VERSION" version
done
```

#### Engine readiness (from repo root)

```bash
# 1. Bump packages/octocode-engine/package.json (16.5.0 is taken → 16.5.1+).
# 2. Propagate to Cargo + all 6 platform package.json. Run MANUALLY — the root is
#    published with --ignore-scripts, which skips the prepublishOnly hook.
yarn workspace @octocodeai/octocode-engine run version:sync

# 3. Rebuild all 6 native targets + TS (see [Build](#build) — `build:all` cross-compiles on one machine):
yarn workspace @octocodeai/octocode-engine run build:all

# 4. Fast gate: version + loader entries + tarball purity + 6 binaries.
yarn workspace @octocodeai/octocode-engine run prepublish:verify

# 5. Full gate: cargo check/fmt/clippy/test/udeps/audit (`dead_code`+`unused_*` are `deny` in `[lints.rust]`) + tsc + vitest + benchmarks.
yarn workspace @octocodeai/octocode-engine run verify

# 6. Prove both entry points load on this host:
cd packages/octocode-engine
node --input-type=module -e "const m=await import('./index.js'); if(typeof m.applyContentViewMinification!=='function')throw Error('ESM broken'); console.log('ESM OK')"
node -e "if(typeof require('./index.cjs').applyContentViewMinification!=='function')throw Error('CJS broken'); console.log('CJS OK')"
cd ../..
```

#### What each check catches

| Check | Catches |
|---|---|
| `version:check` | Cargo / npm / 6-platform version drift |
| `loader:check` | CJS in the ESM entry (the 16.5.0 bug), napi auto-gen drift, `loader/` ↔ root mismatch |
| `pack:check` | a `.node` leaking into the root tarball |
| `platforms:check` | missing / empty / duplicate `.node` in any platform dir |
| `cargo clippy -D warnings`, `cargo fmt --check` | Rust lint / format (`cargo fmt --all` to fix) |
| `cargo check` / `cargo test` | Rust compile + behavior |
| `dead_code` / `unused_*` = `deny` (`[lints.rust]` in `Cargo.toml`) | unused code fails **every** cargo invocation (check/build/clippy), not just clippy — catches dead crates, fields, and functions at compile |
| `cargo +nightly udeps` (`udeps:rust`, part of `verify:rust`) | unused Cargo deps. Prereq (once): `rustup toolchain install nightly && cargo +nightly install cargo-udeps --locked` |
| ESM/CJS load (step 6) + post-publish smoke | entry points failing to import |

> Why this matters: 16.5.0 shipped an auto-gen **CJS** `index.js` under `"type": "module"`, so every ESM `import` threw `require is not defined in ES module scope`. `loader:check` (now in `verify` and `prepublish:verify`) fails the build if that recurs; the `loader/` canonical sources make it self-heal.

#### Monorepo ref hygiene (when releasing more than the engine)

```bash
rg '"workspace:|"file:' packages/*/package.json packages/*/npm/*/package.json
# Expected non-empty matches (NOT blockers):
#   • devDependencies."@octocodeai/octocode-tools-core": "workspace:^" in
#     octocode-mcp and octocode — tools-core is bundled, not published; the ref
#     is a build-time-only link that npm auto-corrects on publish and is never
#     installed by consumers (see "tools-core is bundled, not published").
#   • CLI keeps a dev file: dep on @octocodeai/octocode-core — pin it before
#     publishing the CLI.
# The authoritative gate is each package's prepack guard (runtime deps only):
node packages/octocode-mcp/scripts/check-no-workspace-protocol.mjs
node packages/octocode/scripts/check-no-workspace-protocol.mjs
yarn verify
```

### Publish order

Dependencies must exist on npm before dependents. Publish in this order:

```
1. @octocodeai/octocode-engine npm/{platform} × 6
2. @octocodeai/octocode-engine
3. octocode-mcp
4. octocode
5. octocode-mcp-vscode, when releasing the VS Code extension
6. Homebrew tap update for octocode
```

> `@octocodeai/octocode-tools-core` is **not** in this list — it is bundled into
> `octocode-mcp` and `octocode` at build time, never published. The interface
> packages depend directly on `@octocodeai/octocode-engine` (+ `octocode-core`,
> `octokit`, …), so the engine still publishes first.

### Publish commands

> The engine root uses `--ignore-scripts` (no rebuild at publish), which also skips `prepublishOnly` — so `version:sync` + `prepublish:verify` must already have been run above.

```bash
npm whoami   # confirm auth
ENGINE_VERSION=$(node -p "require('./packages/octocode-engine/package.json').version")

# Dry-run first:
for dir in packages/octocode-engine/npm/*; do
  npm publish "$dir" --access public --provenance --dry-run
done
npm publish packages/octocode-engine          --access public --provenance --ignore-scripts --dry-run
npm publish packages/octocode-mcp             --access public --provenance --ignore-scripts --dry-run
npm publish packages/octocode                 --access public --provenance --dry-run

# Live publish (same commands, drop --dry-run):
for dir in packages/octocode-engine/npm/*; do
  npm publish "$dir" --access public --provenance
done
npm publish packages/octocode-engine          --access public --provenance --ignore-scripts

# Stop here until the freshly published engine root + platform packages resolve.
for pkg in \
  @octocodeai/octocode-engine \
  @octocodeai/octocode-engine-darwin-arm64 \
  @octocodeai/octocode-engine-darwin-x64 \
  @octocodeai/octocode-engine-linux-arm64-gnu \
  @octocodeai/octocode-engine-linux-x64-gnu \
  @octocodeai/octocode-engine-linux-x64-musl \
  @octocodeai/octocode-engine-win32-x64-msvc
do
  npm view "$pkg@$ENGINE_VERSION" version
done

npm publish packages/octocode-mcp             --access public --provenance --ignore-scripts
npm publish packages/octocode                 --access public --provenance
```

> `@octocodeai/octocode-tools-core` is intentionally absent — it is bundled into
> the two interface packages, not published.

### Restore workspace refs after publish

After a full monorepo publish that used `release/sync-packages-version.mjs
--pin-for-publish`, restore local workspace refs so development links siblings
again:

```bash
node release/sync-packages-version.mjs
yarn install
```

For an engine-only release, use the engine's `version:sync`; no monorepo
workspace-ref restore is needed unless you separately pinned interface package
manifests.

Or verify first, then fix:

```bash
node release/sync-packages-local.mjs
node release/sync-packages-local.mjs --fix
yarn install
```

### Smoke test after publish

```bash
tmp=$(mktemp -d) && cd "$tmp" && npm init -y >/dev/null
ENGINE_VERSION=16.5.1
MCP_VERSION=16.2.0
CLI_VERSION=2.0.0
npm install "@octocodeai/mcp@$MCP_VERSION" "octocode@$CLI_VERSION"
npm ls "@octocodeai/octocode-engine@$ENGINE_VERSION"
node --input-type=module -e "const e = await import('@octocodeai/octocode-engine'); console.log('engine:', typeof e.applyContentViewMinification === 'function')"
# tools-core is bundled, not published — it must NOT be installed in node_modules:
test ! -e node_modules/@octocodeai/octocode-tools-core && echo "✓ tools-core not installed (bundled)" || echo "✗ tools-core leaked as a runtime dep"
npx octocode-mcp --help
npx octocode --version
```

### Recovering from a bad interface publish

If a published `octocode` or `octocode-mcp` version depends on
`@octocodeai/octocode-tools-core`, that version is broken by design: tools-core
is unpublished and must stay bundled. Do **not** fix it by publishing
`@octocodeai/octocode-tools-core`.

Recovery sequence:

```bash
# 1. Publish/verify the engine package family first.
ENGINE_VERSION=$(node -p "require('./packages/octocode-engine/package.json').version")
for pkg in \
  @octocodeai/octocode-engine \
  @octocodeai/octocode-engine-darwin-arm64 \
  @octocodeai/octocode-engine-darwin-x64 \
  @octocodeai/octocode-engine-linux-arm64-gnu \
  @octocodeai/octocode-engine-linux-x64-gnu \
  @octocodeai/octocode-engine-linux-x64-musl \
  @octocodeai/octocode-engine-win32-x64-msvc
do
  npm view "$pkg@$ENGINE_VERSION" version
done

# 2. Publish a new patch of the broken interface package from a build where:
#    - package.json has tools-core only in devDependencies
#    - out/dist contains no tools-core import specifier
#    - check-no-workspace-protocol passes
npm publish packages/octocode --access public --provenance

# 3. Mark the broken version so new users do not pick it by accident.
npm deprecate "octocode@<bad-version>" \
  "Broken publish: depended on unpublished @octocodeai/octocode-tools-core. Use a newer patch."
```

---

## Standalone Binaries

Standalone binaries are separate from npm publishing. They compile the MCP server
with Bun and copy native `.node` runtime files next to the executable because
native addons cannot be embedded into the Bun binary. The npm optionalDependency
chain is not used in this path.

The current native runtime is `@octocodeai/octocode-engine`; any legacy
standalone script that still expects a separate security native package must be
removed or updated before cutting a standalone release.

```bash
yarn workspace @octocodeai/mcp run build:bin:darwin-arm64
yarn workspace @octocodeai/mcp run build:bin:darwin-x64
yarn workspace @octocodeai/mcp run build:bin:linux-arm64
yarn workspace @octocodeai/mcp run build:bin:linux-x64
yarn workspace @octocodeai/mcp run build:bin:linux-x64-musl
yarn workspace @octocodeai/mcp run build:bin:windows-x64

cd packages/octocode-mcp/dist
shasum -a 256 octocode-mcp-* > checksums-sha256.txt
```

Upload the 6 binaries + `checksums-sha256.txt` to the GitHub Release for `vX.Y.Z`.

**Layout inside each standalone binary**:

```
dist/
  octocode-mcp-darwin-arm64
  runtime/
    engine/octocode-engine.darwin-arm64.node
```

The loader checks bundled runtime paths before falling back to npm
`optionalDependencies`, so standalone users need no npm install.

---

## Homebrew

**Tap repo:** [`bgauryy/homebrew-octocode`](https://github.com/bgauryy/homebrew-octocode)

The formula installs the published `octocode` npm tarball. Publish `octocode` first, then:

```bash
cd /Users/guybary/Documents/homebrew-octocode
./scripts/update-formula.sh X.Y.Z
brew style Formula/octocode.rb
brew audit --strict --online Formula/octocode.rb
brew uninstall octocode || true
brew install bgauryy/octocode/octocode
octocode --version && octocode tools
```

Homebrew users get native binaries through the same `optionalDependencies` chain: `octocode → @octocodeai/octocode-engine → platform .node` (tools-core is bundled into `octocode`, so it is no longer a link in this chain).

---

## References

- [napi-rs release docs](https://napi.rs/docs/deep-dive/release)
- [npm `os`/`cpu`/`libc` selectors](https://docs.npmjs.com/cli/v11/commands/npm-install/)
- [npm `optionalDependencies`](https://docs.npmjs.com/cli/v11/configuring-npm/package-json/)
