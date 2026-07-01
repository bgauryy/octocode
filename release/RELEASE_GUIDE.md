# Release Guide

> Build, publish, and ship Octocode packages.

---

## Dependency Tree

```
npm install graph (what a user gets)              publish? build input?
─────────────────────────────────────────────────────────────────────
@octocodeai/config                                ❌ NOT published — zero-dep internal
  └─ (none — Node builtins only)                     env/config loader. Bundled by esbuild
                                                     into octocode/octocode-mcp, inlined into
                                                     @octocodeai/pi-extension (dist/env.js +
                                                     skill octocode-config.mjs). devDependency
                                                     everywhere. Users never install it.

@octocodeai/octocode-engine                       ✅ publish
  ├─ optional: @octocodeai/octocode-engine-darwin-arm64    ✅ publish  ← contains one .node binary
  ├─ optional: @octocodeai/octocode-engine-darwin-x64      ✅ publish
  ├─ optional: @octocodeai/octocode-engine-linux-arm64-gnu ✅ publish
  ├─ optional: @octocodeai/octocode-engine-linux-x64-gnu   ✅ publish
  ├─ optional: @octocodeai/octocode-engine-linux-x64-musl  ✅ publish
  ├─ optional: @octocodeai/octocode-engine-win32-x64-msvc  ✅ publish
  └─ runtime deps (installed with engine — power lspGetSemantics tool):
       zod, bash-language-server, intelephense, pyright,
       typescript, typescript-language-server,
       vscode-langservers-extracted, yaml-language-server

@octocodeai/mcp (octocode-mcp)                    ✅ publish
  ├─ @octocodeai/octocode-engine  ──▶  platform .node
  ├─ @octocodeai/octocode-core    (external, sibling repo ^16.1.1)
  ├─ @modelcontextprotocol/sdk
  ├─ @octokit/*, octokit, node-cache, zod
  └─ @octocodeai/octocode-tools-core + @octocodeai/config  ← BUNDLED by esbuild (devDeps), NOT runtime deps

octocode (CLI)                                    ✅ publish
  ├─ @octocodeai/octocode-engine  ──▶  platform .node
  ├─ @octocodeai/octocode-core    (external, sibling repo ^16.1.1)
  ├─ @inquirer/prompts, @octokit/*, octokit, node-cache, open, zod
  └─ @octocodeai/octocode-tools-core + @octocodeai/config  ← BUNDLED by esbuild (devDeps), NOT runtime deps

@octocodeai/pi-extension                          ✅ publish
  ├─ octocode                                        (runtime dep — bundled CLI + version)
  ├─ typebox                                         (optional peerDependency)
  └─ @octocodeai/config  ← BUNDLED (devDep): inlined into dist/env.js AND copied into
       each skill's scripts/ as octocode-config.mjs at build time. NOT a runtime dep —
       the published extension needs nothing from npm for env/config.

octocode-mcp-vscode (VS Code extension)           ✅ publish  (separate release)

@octocodeai/octocode-tools-core                   ❌ NOT published — bundled into
  ├─ @octocodeai/config                              octocode-mcp and octocode
  ├─ @octocodeai/octocode-engine                     at build time via esbuild.
  ├─ @octocodeai/octocode-core                       Users never install it.
  └─ @modelcontextprotocol/sdk, @octokit/*, ...
```

**Platform package selectors** (npm auto-installs the matching one):

| Package | `os` | `cpu` | `libc` |
|---------|------|-------|--------|
| `octocode-engine-darwin-arm64` | darwin | arm64 | — |
| `octocode-engine-darwin-x64` | darwin | x64 | — |
| `octocode-engine-linux-arm64-gnu` | linux | arm64 | glibc |
| `octocode-engine-linux-x64-gnu` | linux | x64 | glibc |
| `octocode-engine-linux-x64-musl` | linux | x64 | musl |
| `octocode-engine-win32-x64-msvc` | win32 | x64 | — |

---

## Publish Order

Must exist on npm **before** dependents. Publish in this order:

```
1. @octocodeai/octocode-engine-{platform}  × 6          (platform .node files)
2. @octocodeai/octocode-engine                           (root loader)
3. @octocodeai/mcp                                       (MCP server)
4. octocode                                              (CLI)
5. @octocodeai/pi-extension                              (Pi harness)
6. octocode-mcp-vscode                                   (VS Code — separate release)
```

> **Never published — bundled at build (devDependencies):**
> `@octocodeai/octocode-tools-core` (esbuild → steps 3 & 4) and
> `@octocodeai/config` (esbuild → octocode/mcp; inlined into pi-extension `dist/env.js` + skill `octocode-config.mjs`).  
> `@octocodeai/octocode-core` EXTERNAL (sibling repo).  
> **Every publishable package runs `check-no-workspace-protocol.mjs` on `prepack`/`prepublishOnly`** — publish aborts if an unpinned `workspace:` ref would ship in a runtime dep field. Pin first with `yarn sync:version:publish`.

---

## Build

### 1. Native engine — all 6 platforms

Each target runs: `prebuild.cjs → napi build --release → postbuild.cjs → tsc`

- `prebuild.cjs` — regenerates `security/patterns.rs` from the canonical TS source
- `napi build` — compiles Rust → `octocode-engine.<triple>.node` + copies to `npm/<platform>/`
- `postbuild.cjs` — restores hand-authored `loader/` files over napi-generated stubs
  (prevents CJS-in-ESM breakage: napi generates CJS `index.js`; the package is `"type":"module"`)
- `tsc` — emits `dist/` (TS wrappers for security + LSP)

```bash
# Full release — cross-compiles all 6 targets on one machine (zig toolchain):
yarn build:native:all

# Verify all 6 .node files landed in npm/<platform>/:
yarn platforms:check

# Single target (dev):
yarn workspace @octocodeai/octocode-engine build:darwin-arm64
```

> **Never** `yarn workspace @octocodeai/octocode-engine build` (host-only) and then publish —
> the other five platform dirs will be empty.

### 2. TS packages

```bash
# All packages in dependency order:
yarn workspaces foreach -pt run build:dev

# Or per-package:
yarn workspace @octocodeai/octocode-tools-core build:dev
yarn workspace @octocodeai/mcp                 build:dev   # rebuilds tools-core first
yarn workspace octocode                        build:dev   # rebuilds tools-core first
```

Build order: `@octocodeai/octocode-engine` → `@octocodeai/octocode-tools-core` → `octocode-mcp` / `octocode`

### 3. Pi extension (harness + skills)

```bash
yarn workspace @octocodeai/pi-extension build      # runs scripts/build.mjs
```

`scripts/build.mjs` assembles a **self-contained** `dist/`:

| Output | From | Notes |
|--------|------|-------|
| `dist/system/APPEND_SYSTEM.md` | `docs/PI/APPEND_SYSTEM.md` | the harness system prompt |
| `dist/skills/` | root `skills/` | minus `octocode`, `octocode-awareness`, `octocode-stats` (SKIPPED_SKILLS) |
| `dist/env.js` | `@octocodeai/config` source | **inlined** — the loader itself, not a re-export |
| `dist/skills/<skill>/scripts/octocode-config.mjs` | `@octocodeai/config` source | **copied into every skill with a `scripts/` dir** (`injectConfigIntoSkills`) so skills load env standalone |
| `dist/web.js` | `src/web.js` | the `web` search/fetch tool |
| `dist/bin/` | `octocode`'s `out/` | bundled CLI |
| `dist/awareness/scripts/` + `schema.json` | root awareness scripts | memory tools + file-lock hooks |

**Key point — `@octocodeai/config` is a build-time (dev) dependency only.** The build inlines it into `dist/env.js` and injects `octocode-config.mjs` into skill `scripts/` dirs, so nothing resolves `@octocodeai/config` from npm at runtime and it need not be published *for pi-extension*. (It is still published for `octocode`/`@octocodeai/mcp`, which externalize declared deps — see Publish Order step 1.)

- **Which skills use it:** only `octocode-brainstorming` (its `serper-search.mjs` / `tavily-search.mjs`) imports `./octocode-config.mjs` (with a local `.env` fallback). The other skills receive the copy but don't import it — harmless and future-proof.
- Build fails loudly if the config-loader source, bundled CLI, prompt, or any skill is missing.

Verify: `yarn workspace @octocodeai/pi-extension verify` (lint + tests + build).

---

## Pre-Publish Checks

### Engine (run before publishing engine packages)

```bash
# 1. Bump version + propagate to Cargo.toml + 6 platform package.json:
yarn workspace @octocodeai/octocode-engine version:sync

# 2. Build all 6 platforms:
yarn build:native:all

# 3. Fast gate (versions, loader entries, tarball purity, 6 binaries):
yarn workspace @octocodeai/octocode-engine prepublish:verify

# 4. Full gate (Rust check/fmt/clippy/test/udeps/audit + TS tests):
yarn workspace @octocodeai/octocode-engine verify

# 5. Smoke — both entry points must load:
node --input-type=module -e "
  const m = await import('./packages/octocode-engine/index.js');
  if (typeof m.applyContentViewMinification !== 'function') throw Error('ESM broken');
  console.log('ESM OK');
"
node -e "
  const m = require('./packages/octocode-engine/index.cjs');
  if (typeof m.applyContentViewMinification !== 'function') throw Error('CJS broken');
  console.log('CJS OK');
"
```

| Check | What it catches |
|---|---|
| `version:sync` | Cargo / npm / 6-platform version drift |
| `loader:check` | CJS in ESM entry (napi auto-gen bug), `loader/` ↔ root mismatch |
| `pack:check` | `.node` leaking into the root tarball |
| `platforms:check` | Missing / empty `.node` in any platform dir |
| `cargo clippy -D warnings` | Rust lint (dead_code/unused_* are `deny` — fail every cargo invocation) |
| ESM/CJS smoke | Entry points failing to import |

### Monorepo packages (run before publishing octocode / octocode-mcp)

```bash
# Pin internal workspace:* refs to exact versions:
node release/sync-packages-version.mjs --pin-for-publish

# Check no workspace: or file: refs leaked into runtime deps (every publishable package
# also runs this automatically on prepack/prepublishOnly — this is the manual pre-check):
node packages/octocode-mcp/scripts/check-no-workspace-protocol.mjs
node packages/octocode/scripts/check-no-workspace-protocol.mjs
node packages/octocode-pi-extension/scripts/check-no-workspace-protocol.mjs
node packages/octocode-agent/scripts/check-no-workspace-protocol.mjs

# Full test + lint gate:
yarn verify
```

> The guard only flags `workspace:` in **published** dep fields (`dependencies`/`optional`/`peer`/`bundled`).
> `workspace:` in `devDependencies` is intentional and allowed — that is how bundled-not-published
> packages (`@octocodeai/config`, `@octocodeai/octocode-tools-core`) are linked at build time.

---

## Publish Commands

```bash
npm whoami   # confirm auth

# ── Engine platform packages (6) ────────────────────────────────────
for dir in packages/octocode-engine/npm/*/; do
  npm publish "$dir" --access public --provenance
done

# ── Engine root ─────────────────────────────────────────────────────
npm publish packages/octocode-engine --access public --provenance --ignore-scripts

# Wait for all 7 engine packages to resolve on the registry:
ENGINE_VERSION=$(node -p "require('./packages/octocode-engine/package.json').version")
for pkg in @octocodeai/octocode-engine{,-darwin-arm64,-darwin-x64,-linux-arm64-gnu,-linux-x64-gnu,-linux-x64-musl,-win32-x64-msvc}; do
  npm view "$pkg@$ENGINE_VERSION" version && echo "  ✅ $pkg"
done

# ── MCP server + CLI ────────────────────────────────────────────────
npm publish packages/octocode-mcp --access public --provenance --ignore-scripts
npm publish packages/octocode    --access public --provenance

# ── Pi extension ────────────────────────────────────────────────────
npm publish packages/octocode-pi-extension --access public --provenance
```

### Restore workspace refs after publish

```bash
node release/sync-packages-version.mjs   # restores workspace:*
yarn install
```

---

## Smoke Test After Publish

```bash
tmp=$(mktemp -d) && cd "$tmp" && npm init -y >/dev/null
npm install octocode @octocodeai/mcp

# Engine resolves + loads:
node --input-type=module -e "
  const e = await import('@octocodeai/octocode-engine');
  console.log('engine ok:', typeof e.applyContentViewMinification === 'function');
"

# tools-core must NOT be installed (it is bundled):
test ! -e node_modules/@octocodeai/octocode-tools-core && echo "✅ tools-core bundled (not installed)" || echo "❌ tools-core leaked"

npx octocode --version
npx octocode-mcp --help
```

---

## Standalone Binaries

Compiled with Bun; the native `.node` is copied next to the executable (not npm-installed).

```bash
yarn workspace @octocodeai/mcp build:bin:darwin-arm64
yarn workspace @octocodeai/mcp build:bin:darwin-x64
yarn workspace @octocodeai/mcp build:bin:linux-arm64
yarn workspace @octocodeai/mcp build:bin:linux-x64
yarn workspace @octocodeai/mcp build:bin:linux-x64-musl
yarn workspace @octocodeai/mcp build:bin:windows-x64

cd packages/octocode-mcp/dist
shasum -a 256 octocode-mcp-* > checksums-sha256.txt
```

Upload 6 binaries + `checksums-sha256.txt` to the GitHub Release.

**Binary layout:**
```
dist/
  octocode-mcp-darwin-arm64
  runtime/engine/octocode-engine.darwin-arm64.node
```

---

## Homebrew

Tap: [`bgauryy/homebrew-octocode`](https://github.com/bgauryy/homebrew-octocode). Publish `octocode` first, then:

```bash
cd /path/to/homebrew-octocode
./scripts/update-formula.sh X.Y.Z
brew install --build-from-source bgauryy/octocode/octocode
octocode --version
```

---

## Dev Workflow

```bash
# First time:
yarn install

# After a publish that pinned versions, restore workspace links:
node release/sync-packages-version.mjs
yarn install

# Build + test one package:
yarn workspace <pkg> build:dev
yarn workspace <pkg> test

# Verify everything:
yarn verify
```

Version tracks are **independent**: engine `16.x`, octocode-mcp `16.x`, octocode `2.x`.

- **Engine-only release**: bump `packages/octocode-engine/package.json` → `yarn workspace @octocodeai/octocode-engine version:sync`
- **Full monorepo release**: `node release/sync-packages-version.mjs` (uses `octocode-mcp` as source)

> Do **not** run `sync-packages-version.mjs` for an engine-only release — it forces every package to the MCP version.
