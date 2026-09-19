# Native consolidation release closure

This runbook tracks the work that remains after consolidating native distribution ownership under `@octocodeai/octocode-native`. It distinguishes code defects that are fixed from release evidence that still requires CI or registry execution.

For the complete publish procedure, artifact contract, and rollback order, see [`packages/octocode-native/docs/PUBLISHING.md`](../packages/octocode-native/docs/PUBLISHING.md).

## Current status

The original local code blockers are resolved:

- The 13-tool MCP catalog serializes to 1,899,345 bytes, below the 2,000,000-byte production limit. Core no longer repeats the full `jevReasoning` and `jevScout` input schemas inside every result continuation union.
- Native contracts identify a clean committed Core revision; `sourceDirty` is `false`, and native provenance verification passes.
- Darwin development builds ad-hoc sign and load-test both local addons after copying them.
- The native CI workflow contains six matching-runner package jobs and a downstream `packages` job that assembles and checks all 24 release artifacts.
- MCP release metadata and pagination-contract coverage include `jevScout`.
- Documentation derives the default catalog correctly: 13 discoverable tools and 10 enabled without clone or Jev credentials.

Observed local verification:

| Check | Result |
|---|---|
| Core lint, typecheck, build, and tests | 174 tests passed at the contract fix |
| Native `verify` | Passed, including 107 Node tests, 229 runtime-library tests, 36 CLI-unit tests, 41 CLI integration tests, and 615 executed engine tests; two manual benchmarks remained ignored |
| Real MCP stdio quick acceptance | 13 checks passed; 13 tools; 1,899,345-byte catalog |
| MCP package `verify` | 173 tests passed with 100% measured adapter coverage |
| CLI `verify` | 119 tests passed |
| Pi unit suite | 2,449 tests passed |
| Workspace health and documentation verification | Passed |

## Release gates

### Gate 1: Darwin ARM64 release package

The clean Core fingerprint changed after the previous staged release binary was built. Rebuild the host release package so the tracked CLI and worker and the generated runtime and engine addons all come from the final contract source and signing scripts.

```bash
yarn workspace @octocodeai/octocode-native build:darwin-arm64
cd packages/octocode-native/npm/darwin-arm64
node ../verify-binary.cjs
```

Then rerun from the repository root:

```bash
yarn workspace @octocodeai/octocode-native verify
yarn workspace @octocodeai/octocode-native pack:check
```

Do not treat the successful development-addon build as release-artifact evidence. The package check must execute the staged release binaries and addons.

### Gate 2: Six-platform CI matrix and aggregate check

A Darwin ARM64 workstation cannot build and execute every supported platform family. Run `.github/workflows/rust-tools-core.yml` in GitHub Actions and require all six `package` matrix entries to pass:

- `darwin-arm64`
- `darwin-x64`
- `linux-arm64-gnu`
- `linux-x64-gnu`
- `linux-x64-musl`
- `win32-x64-msvc`

Each matrix job must run its package's `npm/verify-binary.cjs` on the matching host. The downstream `packages` job must then:

1. download all six platform artifacts;
2. restore executable modes lost by artifact transport;
3. assemble the six `npm/<platform>` directories;
4. pass `platforms:check` for all 24 files;
5. upload `octocode-native-release-packages`.

The local `platforms:check` is expected to fail until those five non-host artifact families are collected. Presence in the aggregate job does not replace execution on each matching runner.

### Gate 3: Core publication order

The fixed Core package version is `19.0.2`; the public registry still reports `19.0.1` as latest. `packages/octocode-mcp/package.json` now requires `@octocodeai/octocode-core@19.0.2`.

Publish and verify Core before publishing MCP or other consumers. npm versions are immutable: if `19.0.2` becomes unavailable or is published with different content, increment Core, and update every exact consumer dependency instead of overwriting an existing version.

After publishing Core, install from the registry in a clean directory and confirm that its discovery catalog contains both `jevReasoning` and `jevScout` and that its output-schema budget remains below 2 MB.

### Gate 4: Integrated repository verification

The focused package checks are green, but the final branch must be tested after the release rebuild, generated-file review, and any concurrent Jev edits settle.

```bash
yarn verify
yarn health:check
yarn docs:verify
node packages/octocode-mcp/tests/integration/stdio.acceptance.mjs --quick
```

If `yarn verify` does not cover a changed package, run that package's `verify` script explicitly. Do not lower coverage ratchets or transport budgets to make a gate pass.

### Gate 5: Generated files and concurrent edits

Before commit or merge, inspect the staged and unstaged changes separately:

```bash
git diff --check
git diff --stat
git diff --cached --stat
git status --short
```

Verify that these generated native contract files share the clean Core revision and fingerprint:

- `packages/octocode-native/crates/runtime/src/contracts/generated/contract-provenance.json`
- `packages/octocode-native/crates/runtime/src/contracts/generated/contracts.rs`
- `packages/octocode-native/crates/runtime/src/contracts/generated/tool-contract.json`

Preserve concurrent Jev work under `skills/octocode-jev-reasoning-loop/` and any independently edited runtime files. Do not silently fold unrelated staged and unstaged changes into the consolidation commit. Rebuild and rerun affected checks after resolving overlaps.

### Gate 6: Registry canary acceptance

After all six platform packages and the native root package are available under a prerelease tag, install them from the registry on every supported platform. Verify:

- `@octocodeai/octocode-native/runtime` loads and reports ABI version 2;
- `@octocodeai/octocode-native/engine` loads independently;
- the native CLI reports the 13-tool catalog;
- the regex worker starts;
- the packaged MCP server starts over stdio;
- the serialized MCP catalog remains below 2 MB;
- representative local read, search, AST, rewrite-preview, and LSP calls preserve their contracts.

Publish platform packages first, then the native root, then CLI, MCP, and Pi consumers. Promote dist-tags only after the canary checks pass. Follow the rollback sequence in the native publishing guide if any registry-installed check fails.

## Completion criteria

The consolidation is release-complete only when all of the following are true:

- [ ] The final Darwin ARM64 release package is rebuilt and executes successfully.
- [ ] All six matching-runner platform jobs pass.
- [ ] The aggregate CI job verifies all 24 artifacts.
- [ ] Core `19.0.2` or a later exact replacement is installed successfully from the registry.
- [ ] Native, CLI, MCP, Pi, workspace-health, and documentation gates pass after final integration.
- [ ] Real MCP stdio behavior and the sub-2-MB catalog gate pass together.
- [ ] Staged and unstaged diffs contain no accidental generated drift or unrelated work.
- [ ] Registry canaries pass on every supported platform before promotion.

Until these checks are complete, the implementation is code-green locally but not fully release-green.
