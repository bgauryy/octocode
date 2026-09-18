# Native and engine distribution consolidation

**Status:** Proposed validation plan; not approved for source moves<br>
**Decision:** Merge npm distribution ownership, not runtime/engine responsibilities<br>
**Evidence snapshot:** 2026-09-18 modified working tree<br>
**Jev readiness review:** Ambiguous viability; more evidence required before source moves<br>
**Target owner:** `packages/octocode-native`

## Summary

Consolidate the native CLI, the `NativeRuntime` Node addon, and the engine primitive Node addon into one npm distribution:

```text
@octocodeai/octocode-native
├── bin/octocode
├── bin/octocode-regex-worker
├── runtime addon                 exposed by . and ./runtime
├── engine primitive addon        exposed by ./engine
└── 6 optional platform packages containing all four artifacts
```

Keep two internal Rust crates and, initially, two Node addons:

```text
packages/octocode-native/
├── Cargo.toml                    virtual workspace
├── crates/
│   ├── engine/                   reusable algorithms and primitive N-API bindings
│   └── runtime/                  tool policy, providers, CLI, and NativeRuntime
├── npm/                          one family of six platform packages
└── package.json                  one public artifact owner
```

Do not move contracts from `@octocodeai/octocode-core`. Do not flatten engine algorithms into runtime policy. Do not combine the addons until measurements show that the reduced duplication is worth the tighter ABI and build coupling.

Retain `packages/octocode-engine` as a temporary, deprecated JavaScript compatibility package that re-exports `@octocodeai/octocode-native/engine`. It must no longer own Rust source or platform packages after cutover.

## Provisional Jev review

An initial `disputed_inference` check was the wrong route because it recast plan viability as a factual readiness claim. It is not used as the governing review. The Jev skill now routes high-cost migration, architecture, and rollout proposals to `decision_review`; `disputed_inference` remains limited to the evidential status of one bounded factual or causal proposition.

After adding the pre-move phase-1B proof, a pinned `jev-1.13.0` `decision_review` evaluated the revised proposal. It returned:

- proposal viability `0.55`, which the runner treats as ambiguous;
- more evidence needed `0.90`;
- unknown external consumers (`R3`) as the primary supplied risk with probability `0.44`.

The runner blocked execution and directed the host to retrieve evidence for the selected risk or assumption. This judgment is advisory, not repository evidence. Public searches can bound external-consumer risk but cannot disprove private or unindexed use. Exact npm lookup reports public engine version `19.1.0`, while the evidence-snapshot working tree declares `19.2.0`; that release-line discrepancy must be reconciled deterministically.

The host decision is to authorize phase zero and evidence-producing phases 1A/1B only. Phase 2 source moves remain blocked until the disposable distribution proof passes, public-consumer audit methods and results are recorded, the compatibility mitigation is approved, and the live/local version discrepancy is resolved.

Corrected review artifacts are stored under `.octocode/octocode-jev-reasoning-loop/run-2026-09-18T19-32-39-154Z-10855/`.

## Why this shape

The current dependency direction already matches the target boundary:

```text
CLI and NativeRuntime
        │
        ▼
octocode-native runtime policy
        │  Rust path dependency
        ▼
octocode-engine primitives
```

The runtime has 74 explicit `octocode_engine::` occurrences across 23 Rust files. Rust LSP resolves representative native calls to the engine's structural, graph, and LSP definitions:

- [`structural_search_files_detailed_filtered`](../packages/octocode-engine/src/portable.rs) from native AST search;
- [`CodeGraphBuilder`](../packages/octocode-engine/src/graph/model.rs) from native topology construction;
- [`NativeLspClient`](../packages/octocode-engine/src/lsp/client.rs) from native LSP orchestration.

There is no reverse Rust dependency. The package merge therefore does not require an implementation merge.

The existing native platform packages already carry the CLI, regex worker, and runtime addon in one package. Adding the engine addon is an extension of that artifact model, not a new packaging mechanism. The duplicate ownership today is the second family of six `@octocodeai/octocode-engine-*` packages.

## Scope

This plan covers:

- Rust workspace and crate relocation;
- root and platform npm package layout;
- CJS, ESM, and TypeScript public entrypoints;
- runtime and engine ABI compatibility;
- MCP, Node CLI, Pi, workspace, documentation, and release rewiring;
- compatibility publishing and deprecation;
- six-platform verification, rollout, and rollback.

## Non-goals

- Moving schemas, descriptions, instructions, or public tool contracts out of `@octocodeai/octocode-core`.
- Combining `NativeRuntime` and all 61 primitive exports into one addon in the first release.
- Redesigning tool behavior, response shapes, security policy, pagination, or provider behavior.
- Removing the Node CLI's interactive installation and skill-materialization responsibilities.
- Claiming that no external engine consumers exist. Public GitHub searches found only same-owner manifests, but private, unindexed, and npm-only consumers remain possible.
- Deleting old engine platform versions from the npm registry.

## Phase-zero gate: isolate the implementation

The evidence snapshot is not a clean baseline. Both target packages and adjacent consumers have concurrent modifications. In particular, engine module exports and native runtime/tool files are changing.

Do not start file moves in the current shared checkout. Before implementation:

1. Identify owners of modifications under:
   - `packages/octocode-native`;
   - `packages/octocode-engine`;
   - `packages/octocode-mcp`;
   - `packages/octocode`;
   - `packages/octocode-pi-extension`;
   - root `package.json`, `yarn.lock`, scripts, and affected docs.
2. Commit or otherwise reconcile those changes. Do not stash another session's work.
3. Create a dedicated branch and worktree from the reconciled commit.
4. Rebuild the native and engine artifacts from source. Do not use the modified checked-in binaries as size or behavior baselines.
5. Record the baseline commit and package versions in the implementation pull request.

**Exit gate:** the implementation worktree is clean, target-package tests pass, and baseline artifacts were built from the recorded commit.

## Compatibility contract

### Public surfaces

| Capability | Current entry | Target entry | Compatibility requirement |
|---|---|---|---|
| Native CLI | `@octocodeai/octocode-native/bin/octocode.cjs` | unchanged | Existing Node delegation resolves unchanged path. |
| Regex worker | `@octocodeai/octocode-native/bin/octocode-regex-worker.cjs` | unchanged | Runtime and direct worker invocation still resolve. |
| Runtime addon | `@octocodeai/octocode-native/native.cjs` | package root and `@octocodeai/octocode-native/runtime` | Keep `./native.cjs` as an explicit export and shim. |
| Engine primitives, CJS | `require('@octocodeai/octocode-engine')` | `require('@octocodeai/octocode-native/engine')` | Compatibility package returns the same binding object shape. |
| Engine primitives, ESM | named/default exports from `@octocodeai/octocode-engine` | named/default exports from `@octocodeai/octocode-native/engine` | Preserve every name in `PUBLIC_NATIVE_EXPORT_NAMES`. |
| Engine types | `@octocodeai/octocode-engine/index.d.ts` | `@octocodeai/octocode-native/engine` types condition | Existing TypeScript fixtures compile unchanged through compatibility package. |
| Tool contracts | `@octocodeai/octocode-core/schema` | unchanged | MCP continues to import core directly. |

### Runtime ABI

Keep `NATIVE_ABI_VERSION` independent from npm versions. The current value is `2`; a package move alone must not increment it.

The runtime addon must continue exposing the N-API camel-case equivalents of:

- `abi_version` and `closed` getters;
- `catalog`;
- `execute`;
- `execute_mcp`;
- `cancel`;
- `close`;
- `store_credentials`;
- `get_credentials`;
- `delete_credentials`;
- `refresh_auth_token`;
- `get_token_with_refresh`.

Preserve structured native error serialization, cancellation behavior, asynchronous close, and drop-triggered `begin_close` behavior.

### Engine API

Use [`packages/octocode-engine/tests/support/nativeExportNames.ts`](../packages/octocode-engine/tests/support/nativeExportNames.ts) as the migration manifest. It currently contains 61 public addon exports. Move the manifest with the engine tests and make it the input to both ESM facade generation and ABI parity tests.

Keep `ENGINE_CORE_API_VERSION` independent from the npm package version. The current value is `1`; relocation does not increment it.

Preserve:

- CJS binding-object exports;
- explicit ESM named exports and default binding export;
- `SUPPORTED_SIGNATURE_EXTENSIONS`, `SUPPORTED_GRAPH_FACT_EXTENSIONS`, and `SUPPORTED_STRUCTURAL_EXTENSIONS` decoration;
- `globalThis.__OCTOCODE_ENGINE_BINDING__` for embedded/SEA runtimes;
- `OCTOCODE_ENGINE_NATIVE_LOAD_FAILED` and diagnostic load attempts;
- the existing TypeScript declarations, including overloads, nullability, and option/result interfaces.

The current declaration file does not declare an ESM default export. Do not silently change that typing contract during relocation.

### Loader independence

The entrypoints must be lazy and independent:

- importing `.` or `./runtime` loads only the runtime addon;
- importing `./engine` loads only the engine addon;
- invoking a CLI launcher loads neither addon into the launcher process;
- a missing engine addon does not prevent CLI or runtime imports when their artifacts exist;
- a missing runtime addon does not prevent engine primitive imports when its artifact exists.

The runtime addon still links engine primitives as an Rust `rlib`; loader independence means it must not also load the separate engine `.node` file.

### Environment overrides

Preserve these existing integration seams:

- `OCTOCODE_NATIVE_BIN` for Node-to-CLI delegation;
- `OCTOCODE_NATIVE_BINDING` for MCP runtime-addon injection;
- `OCTOCODE_REGEX_WORKER` for runtime worker selection;
- `globalThis.__OCTOCODE_ENGINE_BINDING__` for embedded engine bindings.

## Target package layout

```text
packages/octocode-native/
├── Cargo.toml
├── Cargo.lock
├── crates/
│   ├── engine/
│   │   ├── Cargo.toml
│   │   ├── build.rs
│   │   ├── src/
│   │   ├── benches/
│   │   └── tests/
│   └── runtime/
│       ├── Cargo.toml
│       ├── build.rs
│       ├── src/
│       └── tests/
├── bin/
│   ├── octocode.cjs
│   ├── octocode-regex-worker.cjs
│   └── platform.cjs
├── js/
│   ├── runtime.cjs
│   ├── runtime.js
│   ├── runtime.d.ts
│   ├── engine.cjs
│   ├── engine.js
│   └── engine.d.ts
├── native.cjs
├── npm/
│   ├── darwin-arm64/
│   ├── darwin-x64/
│   ├── linux-arm64-gnu/
│   ├── linux-x64-gnu/
│   ├── linux-x64-musl/
│   └── win32-x64-msvc/
├── scripts/
├── tests/
│   ├── engine/
│   ├── runtime/
│   ├── packaging/
│   └── contracts/
├── docs/
│   └── engine/
├── ARCHITECTURE.md
├── README.md
└── package.json
```

`packages/octocode-engine` becomes:

```text
packages/octocode-engine/
├── package.json
├── index.cjs
├── index.js
├── index.d.ts
└── README.md
```

It must contain no Rust source, copied `.node` files, platform-package directories, or independent build pipeline.

## Rust workspace design

Create a virtual workspace at `packages/octocode-native/Cargo.toml`:

```toml
[workspace]
members = ["crates/engine", "crates/runtime"]
default-members = ["crates/runtime"]
resolver = "2"
```

Move release, development, and test profiles from the two package manifests to the workspace root where Cargo will honor them. Keep crate-specific lint policy and features in each crate manifest unless both are demonstrably identical.

### Engine crate

- Keep the Cargo package name `octocode-engine` to avoid rewriting the 74 native call sites and to preserve diagnostic identity.
- Set `publish = false` unless crates.io publishing is introduced deliberately.
- Preserve `rlib` and `cdylib` crate types.
- Preserve the `portable-default`, `napi-addon`, `napi-test`, grammar, CSS, and embedded rewrite features.
- Move `src/build.rs` to the conventional `build.rs` path and update the manifest, or retain the existing path explicitly. Do not lose the macOS `napi-test` dynamic lookup behavior.
- Keep `bindings` behind `napi-addon`; the runtime dependency must continue using engine without N-API.

### Runtime crate

- Keep the Cargo package name `octocode-native` and the `octocode` and `octocode-regex-worker` binary names.
- Set the engine dependency to `path = "../engine"` with the current non-N-API feature set.
- Preserve `rlib` and `cdylib` crate types and the runtime `napi-addon` feature.
- Keep `NATIVE_ABI_VERSION` in this crate.

### Build invocations

Build the artifacts in separate Cargo invocations so engine feature unification does not accidentally add N-API to the runtime's engine dependency:

```bash
cargo build --manifest-path packages/octocode-native/Cargo.toml \
  -p octocode-native --release --bins --no-default-features

cargo build --manifest-path packages/octocode-native/Cargo.toml \
  -p octocode-native --release --lib --features napi-addon

cargo build --manifest-path packages/octocode-native/Cargo.toml \
  -p octocode-engine --release --lib --no-default-features \
  --features portable-default,napi-addon
```

Use equivalent target-specific invocations for all six triples. Verify the exact feature set against the pre-move `cargo tree -e features` output.

## npm entrypoints

Add an explicit export map to `@octocodeai/octocode-native`. The exact paths may differ, but the public shape must be equivalent to:

```json
{
  "type": "module",
  "main": "./js/runtime.cjs",
  "types": "./js/runtime.d.ts",
  "exports": {
    ".": {
      "types": "./js/runtime.d.ts",
      "import": "./js/runtime.js",
      "require": "./js/runtime.cjs"
    },
    "./runtime": {
      "types": "./js/runtime.d.ts",
      "import": "./js/runtime.js",
      "require": "./js/runtime.cjs"
    },
    "./engine": {
      "types": "./js/engine.d.ts",
      "import": "./js/engine.js",
      "require": "./js/engine.cjs"
    },
    "./native.cjs": "./native.cjs",
    "./bin/octocode.cjs": "./bin/octocode.cjs",
    "./bin/octocode-regex-worker.cjs": "./bin/octocode-regex-worker.cjs",
    "./package.json": "./package.json"
  }
}
```

Keep `native.cjs` as a one-line compatibility shim to `./js/runtime.cjs`. Introducing the export map without explicit legacy and bin entries would break the current MCP and CLI `require.resolve` calls.

Generate or check the runtime declarations against the N-API class. Generate the engine ESM facade from the 61-name export manifest, or add a test that fails whenever Rust exports and the hand-authored facade drift.

Do not tighten the root package's Node engine range accidentally. The current CLI, MCP, Pi, and engine packages declare Node `^24.15.0`, while native does not declare a range. Phase zero must record the supported installation matrix and then apply one deliberate root range.

## Platform packages

Retain these six names:

- `@octocodeai/octocode-native-darwin-arm64`;
- `@octocodeai/octocode-native-darwin-x64`;
- `@octocodeai/octocode-native-linux-arm64-gnu`;
- `@octocodeai/octocode-native-linux-x64-gnu`;
- `@octocodeai/octocode-native-linux-x64-musl`;
- `@octocodeai/octocode-native-win32-x64-msvc`.

Each package must contain:

```text
octocode[.exe]
octocode-regex-worker[.exe]
octocode-native.<platform>.node
octocode-engine.<platform>.node
README.md
package.json
```

Give each platform package explicit addon subpaths:

```json
{
  "main": "./octocode-native.<platform>.node",
  "exports": {
    ".": "./octocode-native.<platform>.node",
    "./runtime": "./octocode-native.<platform>.node",
    "./engine": "./octocode-engine.<platform>.node",
    "./package.json": "./package.json"
  }
}
```

The root runtime loader resolves the platform package's `./runtime`; the engine loader resolves `./engine`. Keep local artifact candidates for development and embedded builds, but centralize platform/libc detection in `bin/platform.cjs` so the two loaders cannot drift.

Extend `scripts/copy-binaries.cjs` into a four-artifact copy script. Extend `npm/verify-binary.cjs` and `scripts/check-platform-binaries.cjs` to validate:

- all four files exist;
- binaries have executable bits on Unix;
- both addons can be loaded independently;
- CPU, OS, libc, filenames, exports, and package version match the directory;
- no platform package contains a stale addon from another target.

## Version strategy

Use one `CONSOLIDATED_VERSION` for:

- `@octocodeai/octocode-native`;
- all six native platform packages;
- the compatibility release of `@octocodeai/octocode-engine`.

Adopt the engine's public version lineage rather than keeping native at `0.1.x`. The candidate cutover is the next engine major, for example `20.0.0`, because artifact ownership, package topology, and supported import paths change even though the compatibility package preserves the primitive API.

The exact cutover version is not approved by this document alone. At the evidence snapshot, the working-tree engine manifest is `19.2.0`, but exact npm lookup reports public version `19.1.0`. Phase 1B must reconcile that discrepancy, inspect live registry versions and dist-tags for every root/platform package, audit first-party dependency ranges, and record whether any release automation assumes native's `0.1.x` lineage. The implementation pull request must then record the selected version and rationale before source moves begin.

CLI, MCP, and Pi may retain independent versions, but cutover releases must depend on the exact consolidated version. Do not use a range during the canary because mixed root/platform versions produce loader failures that resemble ABI failures.

Replace engine-specific version scripts with one native-owned consistency script that checks:

- workspace crate versions where a version is required;
- root native version;
- all six platform versions;
- all six native optional-dependency versions;
- compatibility engine version and exact native dependency;
- generated lockfile entries.

Npm versions must not replace `NATIVE_ABI_VERSION` or `ENGINE_CORE_API_VERSION`.

## Implementation phases

### Phase 1A: freeze baseline contracts

Before moving files, add or preserve machine-readable baselines:

1. Save the sorted 61-name engine export manifest.
2. Snapshot CJS keys, ESM keys, and the decorated extension arrays.
3. Add TypeScript fixtures for current named imports, classes, option types, and CJS use.
4. Snapshot `NativeRuntime` property names and assert `abiVersion === 2`.
5. Record `catalog()` tool names, availability fields, server metadata, and contract fingerprints.
6. Record clean release artifact sizes and cold import/startup timings.
7. Record `cargo tree -e features` for both current builds.
8. Pack both root packages and one package for every platform; save tarball file lists.

**Exit gate:** the baseline suite fails if any current public surface is removed or renamed.

### Phase 1B: prove the distribution shape before source moves

Use the clean release artifacts from phase 1A to build disposable package fixtures without changing production manifests or moving Rust source:

1. Create one temporary native platform package containing the current CLI, regex worker, runtime addon, and engine addon.
2. Create a temporary native root with `.`, `./runtime`, `./engine`, `./native.cjs`, and bin exports.
3. Create a JavaScript-only temporary engine compatibility package that depends on the temporary native root.
4. Pack all fixtures and install them into empty CommonJS, ESM, and TypeScript consumer projects.
5. Verify independent runtime/engine loading, all 61 engine exports, runtime ABI `2`, CLI launchers, optional-dependency selection, and failure diagnostics.
6. Compare the packed combined platform size with the sum of the clean current native and engine platform packages.
7. Repeat artifact assembly and executable/addon smoke tests on runners for all six supported targets. These can use CI-produced artifacts; no registry publication is required.
8. Inspect live npm versions and dist-tags for both root packages and all platform packages, reconcile published engine `19.1.0` with the working-tree `19.2.0`, audit first-party ranges, and select the exact consolidated version and compatibility support window.
9. Perform a dated public-consumer audit: repeat GitHub manifest/import searches, inspect npm registry metadata and any available dependent indexes, scan first-party lockfiles and release repositories, and record query scope plus results. Treat zero public matches as bounded evidence only.
10. Approve explicit mitigation for consumers the audit cannot observe: CJS/ESM/type-compatible engine wrapper, unchanged documented paths during the support window, canary notice, and rollback pins. Never make source moves depend on claiming that private consumers do not exist.

This spike is disposable. It must not alter production workspace ownership, checked-in platform binaries, or lockfile topology.

**Exit gate:** an architecture reviewer signs off on the packed-file lists, CJS/ESM/type parity, independent addon loading, all-six-target smoke results, projected size, selected version, and rollback path. If any result invalidates the proposed package shape, revise or reject consolidation before `git mv`.

### Phase 2: move Rust source without changing npm ownership

Use `git mv` so history survives:

| Current | Intermediate/target |
|---|---|
| `packages/octocode-engine/src` | `packages/octocode-native/crates/engine/src` |
| `packages/octocode-engine/benches` | `packages/octocode-native/crates/engine/benches` |
| Engine Rust integration tests | `packages/octocode-native/crates/engine/tests` |
| `packages/octocode-native/src` | `packages/octocode-native/crates/runtime/src` |
| Native Rust integration tests | `packages/octocode-native/crates/runtime/tests` |
| Two Cargo manifests/locks | workspace root plus two crate manifests and one lock |

For a green intermediate commit, allow the existing engine npm scripts to build from the relocated engine manifest. Do not merge npm packages in the same commit as the Rust move.

Run Rust LSP definitions again from the three representative native call sites and confirm they resolve into `crates/engine`. Run Cargo tests, Clippy, formatting, feature-tree comparison, and both addon builds.

**Exit gate:** Rust behavior and both addon ABIs match phase-1A baselines, every phase-1B decision remains valid, and npm entrypoints remain unchanged.

### Phase 3: create unified platform payloads

1. Extend native artifact copy and verification scripts.
2. Copy the engine addon into each native platform package.
3. Add platform `./runtime` and `./engine` exports.
4. During this phase only, continue producing old engine platform packages from the same engine addon so current engine entrypoints remain testable.
5. Add clean-install tests for each supported platform in CI.

**Exit gate:** each native platform package is self-sufficient and both addons load independently.

### Phase 4: add native root subpaths

1. Move engine loader sources, declarations, ESM facade, and Node ABI tests under native ownership.
2. Add `.`/`./runtime` and `./engine` export conditions.
3. Preserve `./native.cjs` and bin subpaths explicitly.
4. Use one shared platform detector.
5. Add lazy-loading and missing-sibling-artifact tests.
6. Move engine docs to `packages/octocode-native/docs/engine` and add redirects or corrected links.

**Exit gate:** new native imports pass every phase-1A engine/runtime contract without using `@octocodeai/octocode-engine`.

### Phase 5: convert engine to a compatibility package

Rewrite `packages/octocode-engine` as JavaScript-only:

```js
// index.cjs
module.exports = require('@octocodeai/octocode-native/engine');
```

```js
// index.js
export * from '@octocodeai/octocode-native/engine';
export { default } from '@octocodeai/octocode-native/engine';
```

Its declaration entry re-exports the native engine declarations. Keep the current package name, license, repository metadata, Node range, and ESM/CJS conditions. Depend on the exact consolidated native version.

Remove from the compatibility package:

- `Cargo.toml`, `Cargo.lock`, `src`, `benches`, and Rust tests;
- `npm/*` platform packages;
- N-API build and postbuild scripts;
- optional engine platform dependencies;
- independent binary, loader, ABI, and version scripts now owned by native.

Run old-import/new-import identity and parity tests in separate processes so module caches do not hide loader mistakes.

**Exit gate:** existing CJS, ESM, and TypeScript consumers work by installing only the compatibility root and its native dependency.

### Phase 6: rewire first-party consumers

#### MCP

In [`packages/octocode-mcp/src/native/index.mjs`](../packages/octocode-mcp/src/native/index.mjs):

- resolve `@octocodeai/octocode-native/runtime` by default;
- keep `OCTOCODE_NATIVE_BINDING` injection;
- keep the `NativeRuntime` constructor check;
- keep `@octocodeai/octocode-core/schema` unchanged;
- preserve catalog registration, cancellation, `executeMcp`, close, and signal handling.

The AST dependent graph identifies `src/index.ts`, `src/public.ts`, `tests/native/node-boundary.mjs`, and their tests as the immediate MCP validation surface.

#### Node CLI

Keep `@octocodeai/octocode-native/bin/octocode.cjs` stable. The AST dependent graph identifies `src/cli/index.ts`, `src/cli/interactive-install.ts`, and `tests/cli/native-delegate.test.ts` as direct consumers of the delegation boundary.

Preserve:

- explicit `OCTOCODE_NATIVE_BIN` precedence;
- fail-closed behavior when native is unavailable;
- Node ownership of `skill` and interactive installation;
- environment and stdio passthrough;
- direct executable versus `.cjs` launcher handling.

#### Pi extension

The repository search found the engine package only in Pi's `package.json`, not in scanned `src`, `scripts`, or tests. Remove the direct engine dependency and run the full Pi build/test suite. Add a native dependency only if package tracing proves a real direct runtime need; do not retain a manifest-only dependency defensively.

#### Workspace and release scripts

Update at least:

- root `package.json` resolutions, workspaces, `build:ci`, `build:native:all`, `platforms:check`, and CI exclusions;
- `scripts/dev-resolution-contract.mjs`;
- `scripts/dev-setup.mjs`;
- `scripts/prepublish.mjs`;
- `packages/octocode/scripts/check-no-workspace-protocol.mjs`;
- engine/native version and platform scripts;
- package build/prepack scripts and `yarn.lock`.

The root workspace must include `packages/octocode-native/npm/*` and must no longer include `packages/octocode-engine/npm/*` after cutover.

#### Documentation

Update package ownership and links in:

- `AGENTS.md`;
- `README.md`;
- `docs/README.md`;
- `docs/PACKAGES.md`;
- `docs/OCTOCODE_TOOLS.md`;
- `docs/OCTOCODE_MCP.md`;
- `docs/SECURITY.md`;
- `docs/TOOL_DATA_CONTRACT.md`;
- engine LSP/language links and package READMEs;
- package architecture documents and `scripts/README.md`;
- this plan's evidence links after the source move, or an explicit archival note if the plan is frozen as a historical snapshot.

Describe engine as an internal crate and `./engine` public subpath, not as an independent platform distribution.

**Exit gate:** a repository search has no active standalone-engine ownership references outside the compatibility package, migration notes, lockfile history, and intentionally preserved old-version documentation.

### Phase 7: end-to-end acceptance

Run the verification matrix below from clean artifacts. Do not accept compile-only evidence.

## Verification matrix

### Rust

- `cargo fmt --all -- --check` at the native workspace root.
- Clippy for both crates, all targets, and intended feature combinations.
- Runtime tests with engine portable features and no engine N-API feature.
- Engine Rust tests with default features.
- Engine N-API tests, including macOS `napi-test` behavior.
- Benchmark compilation.
- `cargo tree --duplicates` and feature-tree comparison.
- Rust audit and unused-dependency checks without lowering existing gates.

### Node API

Test CJS and ESM in fresh processes:

```js
require('@octocodeai/octocode-native');
require('@octocodeai/octocode-native/runtime');
require('@octocodeai/octocode-native/native.cjs');
require('@octocodeai/octocode-native/engine');
require('@octocodeai/octocode-engine');
```

Also test dynamic ESM imports for native runtime, native engine, and compatibility engine.

Assertions:

- all runtime paths expose the same `NativeRuntime` constructor contract;
- runtime ABI remains `2`;
- native engine and compatibility engine expose all 61 manifest names;
- CJS and ESM values are behaviorally equivalent;
- extension arrays are frozen and sorted as before;
- engine loader failure retains code and diagnostics;
- TypeScript fixtures compile under NodeNext and CommonJS-oriented consumer configurations;
- importing one surface does not require the sibling addon.

### CLI

Test both paths:

1. Execute the platform `octocode` binary directly.
2. Execute `node packages/octocode/out/octocode.js` and verify delegation.

Cover:

- `--help` and version;
- `context`;
- `tools --json`;
- at least one local search, AST, and LSP command;
- regex worker discovery;
- auth/config commands that do not require network mutation;
- missing binary and explicit `OCTOCODE_NATIVE_BIN` behavior;
- exit-code and signal propagation.

### MCP

Run a real stdio session, not only adapter unit tests:

- initialize;
- `tools/list` and contract fingerprint comparison;
- execute a local tool;
- execute a bulk request;
- cancel an in-flight request;
- close by protocol, stdin end, SIGINT, and SIGTERM;
- verify no TypeScript tool-execution fallback appears.

### Platform CI

Verify these exact targets:

| npm suffix | Rust target |
|---|---|
| `darwin-arm64` | `aarch64-apple-darwin` |
| `darwin-x64` | `x86_64-apple-darwin` |
| `linux-arm64-gnu` | `aarch64-unknown-linux-gnu` |
| `linux-x64-gnu` | `x86_64-unknown-linux-gnu` |
| `linux-x64-musl` | `x86_64-unknown-linux-musl` |
| `win32-x64-msvc` | `x86_64-pc-windows-msvc` |

For every target, install the packed root and platform tarballs into an empty project with scripts disabled first, inspect the selected optional dependency, then enable and exercise the CLI and both addons.

### Packaging and performance

From clean release builds:

- compare each unified platform tarball against the sum of the corresponding old native and engine tarballs;
- fail if unexplained overhead exceeds 5% beyond that sum;
- compare cold runtime import, cold engine import, direct CLI startup, and Node-delegated CLI startup;
- investigate regressions above 10% or 25 ms, whichever is larger;
- verify runtime imports do not load the engine addon file;
- verify tarballs exclude `target`, source-only tests, caches, and old platform artifacts.

The separate-addons design intentionally retains some duplicate engine machine code. Record it. Do not collapse addons in this change solely to improve the package-size result.

## Release plan

1. Publish all six consolidated native platform packages under a prerelease version and `next` tag.
2. Publish the native root package at the same version and tag.
3. Run clean installs and the complete acceptance matrix from the registry artifacts.
4. Publish the compatibility engine package with an exact dependency on that native version.
5. Publish canary MCP, CLI, and Pi releases using exact native dependencies.
6. Soak the canary on all six platforms.
7. Promote platform packages, native root, compatibility engine, then consumers to `latest` in that order.
8. Add an npm deprecation message to the compatibility engine cutover line directing users to `@octocodeai/octocode-native/engine`. Keep it functional for at least two minor release cycles or 90 days, whichever is longer.
9. Stop publishing new `@octocodeai/octocode-engine-*` platform versions. Do not deprecate or remove versions still required by pre-cutover engine releases.

Example migration message:

```text
@octocodeai/octocode-engine has moved to @octocodeai/octocode-native/engine.
This compatibility package remains functional during the migration window.
```

## Rollback plan

Npm artifacts are immutable; rollback means changing dist-tags or publishing a corrective version, not overwriting packages.

Before promotion:

- preserve previous `latest` versions and all old engine platform packages;
- record every previous dist-tag target;
- keep the pre-cutover branch and release artifacts;
- do not remove old loader candidates.

If canary fails:

1. Leave `latest` unchanged.
2. Publish a corrected prerelease or abandon the canary version.
3. Pin first-party canary consumers back to their prior native/engine dependencies.

If failure appears after promotion:

1. Point consumer dist-tags back to known-good releases first.
2. Restore native and engine root dist-tags where dependency constraints permit.
3. Publish a compatibility patch if the new engine wrapper cannot resolve the restored native line.
4. Keep consolidated platform packages available for forensic reproduction.
5. Re-run the registry-install acceptance suite before re-promotion.

The Rust source move is independently reversible with Git because phases 2–5 are separate commits. Do not combine the move, loader cutover, consumer migration, and deprecation in one commit or release step.

## Risks and mitigations

| Risk | Mitigation |
|---|---|
| Concurrent target-package edits are lost during moves. | Hard phase-zero clean-worktree gate and dedicated worktree. |
| Private or npm-only primitive consumers break. | Full compatibility package, unchanged 61-name API, migration window, canary. |
| Export maps block legacy deep paths. | Explicit `./native.cjs` and bin exports with direct resolution tests. |
| One platform package becomes materially larger. | Baseline sum, 5% unexplained-overhead gate, keep two addons initially. |
| Runtime and engine loaders select different libc targets. | One shared platform detector and target-matrix tests. |
| Cargo feature unification links N-API into runtime engine code. | Separate package-specific Cargo invocations and feature-tree assertions. |
| ESM facade drifts from Rust exports. | One 61-name manifest drives generation or mandatory parity checks. |
| Compatibility wrapper hides loader identity problems through module cache. | Separate-process old/new import tests and failure-path tests. |
| AST graph under-reports macro or dynamic loader edges. | LSP definition checks, Cargo tests, runtime loader tests, and no deletion based only on AST. |
| Pi retains a stale transitive dependency. | Remove manifest-only engine dependency and verify full packaged Pi build. |
| Version mismatch resembles an ABI failure. | One version source, exact canary dependencies, release-order enforcement. |

## Evidence quality and limits

The implementation plan used both AST and LSP evidence:

- AST filesystem inventories completed for both packages.
- AST mapped direct MCP and CLI dependents.
- AST extracted the engine crate root and 41-function portable facade.
- AST reported no engine runtime cycles and no native runtime cycles, but native scanning skipped one file.
- Rust LSP semantically resolved representative native calls across the path dependency into engine definitions.
- TypeScript LSP resolved the MCP construction function locally; the CLI TypeScript server closed repeatedly, so CLI dependent claims rely on the complete AST import graph and exact source reads.

AST diagnostics marked Rust macro expansion and dynamic CommonJS requires as unsupported. Accordingly, graph results are migration candidates and coverage evidence, not proof that unreported edges do not exist.

## Completion checklist

- [ ] Phase-zero ownership and clean-worktree gate completed.
- [ ] Baseline commit, versions, ABI manifests, feature trees, sizes, and timings recorded.
- [ ] Disposable combined packages pass phase-1B CJS, ESM, type, loader, size, and six-target checks.
- [ ] Published engine `19.1.0` versus working-tree `19.2.0` is reconciled.
- [ ] Exact cutover version and compatibility window are approved from live registry and consumer-range evidence.
- [ ] Dated public-consumer audit and explicit unknown-consumer mitigation are approved.
- [ ] Engine and runtime live as separate crates under native ownership.
- [ ] Rust LSP representative definitions resolve after the move.
- [ ] One Cargo lock and workspace build pipeline replaces two.
- [ ] Six native platform packages contain CLI, worker, runtime addon, and engine addon.
- [ ] Native root exposes `.`, `./runtime`, `./engine`, legacy shim, and bin paths.
- [ ] Runtime ABI `2` and engine core API `1` remain stable unless separately justified.
- [ ] All 61 engine exports pass CJS, ESM, type, and behavior parity tests.
- [ ] `@octocodeai/octocode-engine` is JavaScript-only and re-exports native `/engine`.
- [ ] MCP uses `/runtime` and still imports contracts from core.
- [ ] Node CLI delegates through the unchanged bin path.
- [ ] Pi's manifest-only engine dependency is removed or justified by traced use.
- [ ] Root resolutions, workspaces, scripts, lockfile, release checks, and docs are updated.
- [ ] Direct CLI, delegated CLI, real stdio MCP, runtime addon, and engine addon pass.
- [ ] All six registry-install platform jobs pass.
- [ ] Package size, cold load, startup, and lazy-loading gates pass.
- [ ] Canary, promotion order, deprecation window, and rollback metadata are recorded.
