# Octocode LSP Full Migration Plan

**Status:** Proposed  
**Owner:** Octocode MCP / LSP maintainers  
**Goal:** remove the MCP-local LSP runtime from `packages/octocode-mcp` and make
`packages/octocode-lsp` the only implementation of LSP process management,
configuration, pooling, protocol operations, and benchmarks.

## Decision

`octocode-lsp` is the source of truth. `octocode-mcp` keeps only the
agent-facing `lspGetSemantics` MCP tool: schemas, hints, evidence
envelopes, pagination, compact formatting, and tool registration.

The MCP package must not contain a local `src/lsp` runtime directory, LSP
protocol dependencies, TypeScript language-server dependencies, or runtime
tests for LSP internals.

## Current State

`packages/octocode-mcp/src/lsp` currently contains compatibility re-export
files for `octocode-lsp` subpaths:

```text
packages/octocode-mcp/src/lsp/cancellableRequest.ts
packages/octocode-mcp/src/lsp/client.ts
packages/octocode-mcp/src/lsp/config.ts
packages/octocode-mcp/src/lsp/evidence.ts
packages/octocode-mcp/src/lsp/initConstants.ts
packages/octocode-mcp/src/lsp/initParams.ts
packages/octocode-mcp/src/lsp/lspClientPool.ts
packages/octocode-mcp/src/lsp/lspDocumentManager.ts
packages/octocode-mcp/src/lsp/lspErrorCodes.ts
packages/octocode-mcp/src/lsp/lspOperations.ts
packages/octocode-mcp/src/lsp/lspRegistry.ts
packages/octocode-mcp/src/lsp/manager.ts
packages/octocode-mcp/src/lsp/resolver.ts
packages/octocode-mcp/src/lsp/schemas.ts
packages/octocode-mcp/src/lsp/symbols.ts
packages/octocode-mcp/src/lsp/types.ts
packages/octocode-mcp/src/lsp/uri.ts
packages/octocode-mcp/src/lsp/validation.ts
packages/octocode-mcp/src/lsp/workspaceRoot.ts
```

These files are temporary migration shims. They keep old MCP imports alive but
also preserve an ownership ambiguity: LSP appears to exist in both packages.

`packages/octocode-mcp/tests/lsp` still tests LSP internals from the MCP
package. Those tests belong either in `packages/octocode-lsp/tests` or should be
deleted when already covered by `octocode-lsp` tests and benchmarks.

## Target State

`packages/octocode-mcp`:

- has no `src/lsp` directory;
- imports LSP runtime APIs directly from `octocode-lsp/*`;
- keeps `src/tools/lsp/**` because that is the MCP tool layer;
- keeps MCP tool tests for `lspGetSemantics`;
- removes LSP runtime tests from `tests/lsp`;
- depends on `octocode-lsp` but not on protocol/server packages used only by
  the runtime.

`packages/octocode-lsp`:

- owns all LSP runtime implementation and tests;
- owns the language-server registry and provider selection;
- owns real benchmark fixtures under `benchmark/{lang}`;
- exposes stable subpath exports used by MCP tool code.

## Architecture Boundary

Keep this split:

| Package | Owns | Must not own |
|---|---|---|
| `octocode-lsp` | stdio process lifecycle, `LSPClient`, pooling, registry, provider resolution, document sync, semantic operations, URI/path helpers, runtime benchmarks | MCP schemas, MCP response envelopes, tool registration |
| `octocode-mcp` | `lspGetSemantics` schema, bulk execution, security wrapper integration, hints, compact output, evidence payloads, tool registration | LSP client internals, JSON-RPC protocol wiring, language-server binaries |

This preserves the MCP surface while removing duplicate runtime ownership.

## Phase 1 — Rewrite MCP Tool Imports

Change MCP tool imports from `../../../lsp/*` to `octocode-lsp/*`.

Required import rewrites:

```text
packages/octocode-mcp/src/tools/lsp/semantic_content/execution.ts
  ../../../lsp/manager.js -> octocode-lsp/manager
  ../../../lsp/workspaceRoot.js -> octocode-lsp/workspaceRoot
  ../../../lsp/types.js -> octocode-lsp/types

packages/octocode-mcp/src/tools/lsp/shared/semanticTypes.ts
  ../../../lsp/types.js -> octocode-lsp/types

packages/octocode-mcp/src/tools/lsp/shared/resolveSymbolAnchor.ts
  ../../../lsp/resolver.js -> octocode-lsp/resolver
  ../../../lsp/types.js -> octocode-lsp/types
  ../../../lsp/lspErrorCodes.js -> octocode-lsp/lspErrorCodes

packages/octocode-mcp/src/tools/lsp/shared/callHierarchyTraversal.ts
  ../../../lsp/client.js -> octocode-lsp/client
  ../../../lsp/types.js -> octocode-lsp/types
  ../../../lsp/validation.js -> octocode-lsp/validation
```

After this phase, `packages/octocode-mcp/src/tools/lsp/**` must not import from
`packages/octocode-mcp/src/lsp/**`.

Acceptance check:

```bash
rg -n "from ['\\\"].*\\.\\./.*lsp|from ['\\\"]../../../lsp|from ['\\\"]../../lsp" packages/octocode-mcp/src
```

Expected result: no imports from MCP-local `src/lsp`.

## Phase 2 — Rewrite MCP Tool Tests

Keep tests that verify MCP behavior, but mock/import `octocode-lsp` directly.

Required test rewrites:

```text
packages/octocode-mcp/tests/tools/lsp_new_public_execution.test.ts
  vi.mock('../../src/lsp/manager.js') -> vi.mock('octocode-lsp/manager')
  vi.mock('../../src/lsp/workspaceRoot.js') -> vi.mock('octocode-lsp/workspaceRoot')
  import from ../../src/lsp/manager.js -> octocode-lsp/manager

packages/octocode-mcp/tests/tools/lsp_workspace_root_routing.test.ts
  import ../../src/lsp/manager.js -> octocode-lsp/manager

packages/octocode-mcp/tests/tools/tool_execution_branches.test.ts
  vi.mock('../../src/lsp/manager.js') -> vi.mock('octocode-lsp/manager')

packages/octocode-mcp/tests/tools/local_lsp_stats_runtime_contract.test.ts
  vi.mock('../../src/lsp/manager.js') -> vi.mock('octocode-lsp/manager')
  vi.mock('../../src/lsp/workspaceRoot.js') -> vi.mock('octocode-lsp/workspaceRoot')

packages/octocode-mcp/tests/tools/callHierarchyTraversal.test.ts
  type imports from ../../src/lsp/types.js -> octocode-lsp/types
```

Keep these test families in `octocode-mcp`:

- `tests/tools/lsp_new_public_execution.test.ts`
- `tests/tools/lsp_new_public_tools.test.ts`
- `tests/tools/lsp_workspace_root_routing.test.ts`
- `tests/tools/local_lsp_stats_runtime_contract.test.ts`
- `tests/tools/callHierarchyTraversal.test.ts`
- hint/schema/output tests that verify the MCP tool contract

These tests validate MCP behavior, not the LSP runtime.

## Phase 3 — Move or Delete Runtime Tests

Move LSP runtime tests from `packages/octocode-mcp/tests/lsp` to
`packages/octocode-lsp/tests` only when they still add coverage beyond existing
`octocode-lsp` tests and `benchmark/run.mjs`.

Runtime tests to migrate or delete:

```text
packages/octocode-mcp/tests/lsp/client.branches.test.ts
packages/octocode-mcp/tests/lsp/client.command.test.ts
packages/octocode-mcp/tests/lsp/client.coverage.test.ts
packages/octocode-mcp/tests/lsp/client.handler.test.ts
packages/octocode-mcp/tests/lsp/client.test.ts
packages/octocode-mcp/tests/lsp/client.uri.test.ts
packages/octocode-mcp/tests/lsp/config.test.ts
packages/octocode-mcp/tests/lsp/evidence.test.ts
packages/octocode-mcp/tests/lsp/lspDocumentManager.test.ts
packages/octocode-mcp/tests/lsp/lsp_best_practices.test.ts
packages/octocode-mcp/tests/lsp/lsp_cancel_on_timeout.test.ts
packages/octocode-mcp/tests/lsp/lsp_error_codes_wired.test.ts
packages/octocode-mcp/tests/lsp/lsp_pool.test.ts
packages/octocode-mcp/tests/lsp/lsp_status_restart.test.ts
packages/octocode-mcp/tests/lsp/manager.branches.test.ts
packages/octocode-mcp/tests/lsp/resolveSymbolAnchor.test.ts
packages/octocode-mcp/tests/lsp/resolver.identifierchar.test.ts
packages/octocode-mcp/tests/lsp/resolver.test.ts
packages/octocode-mcp/tests/lsp/runtime-deps.test.ts
packages/octocode-mcp/tests/lsp/symbols.test.ts
packages/octocode-mcp/tests/lsp/uri.safe.test.ts
packages/octocode-mcp/tests/lsp/validation.test.ts
packages/octocode-mcp/tests/lsp/workspaceRoot.test.ts
```

Recommended handling:

| Test group | Destination |
|---|---|
| `client*`, `lspDocumentManager`, `lsp_cancel_on_timeout` | move to `packages/octocode-lsp/tests` |
| `config`, `manager`, `runtime-deps`, `workspaceRoot` | move to `packages/octocode-lsp/tests` and update package assertions |
| `resolver*`, `symbols`, `uri.safe`, `validation`, `evidence`, `lsp_pool`, `lsp_error_codes_wired` | move to `packages/octocode-lsp/tests` unless already covered |
| `resolveSymbolAnchor.test.ts` | split: resolver behavior to `octocode-lsp`, MCP anchor-envelope behavior to MCP tool tests |

After migration, delete the entire `packages/octocode-mcp/tests/lsp` directory.

Acceptance check:

```bash
test ! -d packages/octocode-mcp/tests/lsp
```

## Phase 4 — Delete MCP-Local Runtime Shims

After Phase 1 and Phase 2 have removed all imports, delete:

```text
packages/octocode-mcp/src/lsp/
```

Do not delete:

```text
packages/octocode-mcp/src/tools/lsp/
```

The `src/tools/lsp` directory is the MCP tool adapter and remains owned by
`octocode-mcp`.

Acceptance checks:

```bash
test ! -d packages/octocode-mcp/src/lsp
rg -n "src/lsp|\\.\\./\\.\\./src/lsp|\\.\\./\\.\\./\\.\\./lsp|from ['\\\"].*lsp/" packages/octocode-mcp/src packages/octocode-mcp/tests
```

The `rg` command may still show `src/tools/lsp` imports. It must not show
`src/lsp` imports.

## Phase 5 — Remove MCP LSP Runtime Dependencies

Keep:

```json
"octocode-lsp": "workspace:^"
```

Remove from `packages/octocode-mcp/package.json` if no non-LSP references remain:

```json
"typescript": "^5.9.3",
"typescript-language-server": "^5.1.3",
"vscode-jsonrpc": "^8.2.1",
"vscode-languageserver-protocol": "^3.17.5",
"vscode-uri": "^3.1.0"
```

Rationale:

- `octocode-lsp` owns `typescript`, `typescript-language-server`,
  `vscode-jsonrpc`, `vscode-languageserver-protocol`, and `vscode-uri`.
- `octocode-mcp` should consume LSP through `octocode-lsp` exports only.
- MCP package tests should not need protocol packages after runtime tests move.

Run:

```bash
node .yarn/releases/yarn-4.9.1.cjs install
```

Then verify `yarn.lock` only keeps these packages through `octocode-lsp` or
other legitimate non-MCP owners.

## Phase 6 — Package and Build Cleanup

Check generated declarations and build outputs:

```bash
node .yarn/releases/yarn-4.9.1.cjs workspace octocode-lsp build
node .yarn/releases/yarn-4.9.1.cjs workspace octocode-mcp build:dev
```

Post-build assertions:

```bash
test ! -d packages/octocode-mcp/dist/lsp
rg -n "octocode-mcp/dist/lsp|src/lsp" packages/octocode-mcp/dist packages/octocode-mcp/dist/public.d.ts
```

If `dist/lsp` exists after a clean build, check `tsconfig.build.json` and import
paths for remaining MCP-local LSP references.

## Phase 7 — Documentation Updates

Update documentation to describe the split:

- `docs/dev/reference/LSP_TOOLS_REFERENCE.md`
- `docs/dev/reference/LOCAL_TOOLS_REFERENCE.md`
- `docs/dev/DEVELOPMENT_GUIDE.md`
- `docs/README.md` if it lists package ownership

Documentation rule: any Markdown links added to docs must use absolute GitHub
URLs with this base:

```text
https://github.com/bgauryy/octocode-mcp/blob/main/
```

Example:

```markdown
[LSP Tools Reference](https://github.com/bgauryy/octocode-mcp/blob/main/docs/dev/reference/LSP_TOOLS_REFERENCE.md)
```

## Verification Gate

Required commands:

```bash
node .yarn/releases/yarn-4.9.1.cjs workspace octocode-lsp typecheck
node .yarn/releases/yarn-4.9.1.cjs workspace octocode-lsp lint
node .yarn/releases/yarn-4.9.1.cjs workspace octocode-lsp build
node packages/octocode-lsp/benchmark/run.mjs

node .yarn/releases/yarn-4.9.1.cjs workspace octocode-mcp typecheck
node .yarn/releases/yarn-4.9.1.cjs workspace octocode-mcp lint
node .yarn/releases/yarn-4.9.1.cjs workspace octocode-mcp test:contracts
```

Expected benchmark posture:

- TypeScript must pass all seven semantic operations.
- JavaScript must pass all applicable semantic operations.
- Missing language servers must be reported as `SKIP`, not failure.
- Broken language servers must fail with stderr.
- Per-language unsupported capabilities must stay visible and documented.

## Rollback

Rollback is simple until Phase 4:

1. Restore MCP imports to `../../../lsp/*`.
2. Restore `packages/octocode-mcp/src/lsp` shims.
3. Restore `packages/octocode-mcp/tests/lsp`.
4. Restore removed package dependencies.
5. Run `octocode-mcp typecheck` and `test:contracts`.

After Phase 4, rollback should use Git revert instead of recreating files by
hand.

## Done Criteria

The migration is complete when all are true:

- `packages/octocode-mcp/src/lsp` does not exist.
- `packages/octocode-mcp/tests/lsp` does not exist.
- `packages/octocode-mcp/src/tools/lsp/**` imports `octocode-lsp/*` directly.
- `packages/octocode-mcp/package.json` has no LSP runtime/protocol dependencies
  except `octocode-lsp`.
- `octocode-lsp` owns runtime tests and real benchmarks.
- `octocode-mcp` tests only the MCP tool contract for LSP.
- `octocode-mcp typecheck`, `octocode-mcp test:contracts`, `octocode-lsp
  typecheck`, `octocode-lsp lint`, `octocode-lsp build`, and the LSP benchmark
  have been run and their results are recorded.
