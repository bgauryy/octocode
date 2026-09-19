# RFC: Tool-Taxonomy & Contract Drift Between `octocode-tools-core` and `octocode-native`

- Status: Draft / for decision
- Date: 2026-09-19
- Scope: how tool names, schemas, and limits are defined in the TS granular server (A) vs the Rust consolidated engine (B), and how to make them stop drifting.
- Decision requested: pick a single taxonomy+limits authority and the smallest first step.

## 1. Context & problem

Two shipping implementations expose overlapping capabilities through different tool surfaces and different schema authorities.

- **A — `packages/octocode-tools-core`** (TypeScript, `main`): 14 granular tools, one folder each under `src/tools/`, verified live: `github_clone_repo, github_fetch_content, github_search_code, github_search_discussions, github_search_pull_requests, github_search_repos, github_view_repo_structure, local_dead_code, local_fetch_content, local_find_files, local_ripgrep, local_view_structure, lsp, package_search` (ghSearch tree of `packages/octocode-tools-core/src/tools` @ `main`).
- **B — `packages/octocode-native`** (Rust): 12 consolidated tools, verified from the registered MCP surface: `ghSearch, ghGetFileContent, ghSearchHistory, ghGetHistoryItem, ghCloneRepo, artifactSearch, localSearch, localFetch, astSearch, astRewrite, lspSearch, jevReasoning`.

The two surfaces are close but not congruent, and — critically — they resolve their **schemas and numeric limits from two independent authorities** that are not linked by any check. They happen to agree today; nothing keeps them agreeing.

## 2. Tool-name mapping A ↔ B

| A (granular, tools-core `main`) | B (consolidated, native) | Notes |
|---|---|---|
| `github_search_code` | `ghSearch` `{operation:"code"}` | 1:N collapse — A splits by resource, B by `operation` enum |
| `github_search_repos` | `ghSearch` `{operation:"repositories"}` | same |
| `github_view_repo_structure` | `ghSearch` `{operation:"tree"}` | same |
| `github_fetch_content` | `ghGetFileContent` | 1:1 |
| `github_search_pull_requests` | `ghSearchHistory` (+ `ghGetHistoryItem` for one item) | A's PR/issue search folds into a generic "history" search + a granular fetch |
| `github_search_discussions` | `ghSearchHistory` (**capability gap — unverified in B**) | No discussions-specific op confirmed in the native surface; flag for confirmation |
| `github_clone_repo` | `ghCloneRepo` | 1:1 |
| `local_ripgrep` | `localSearch` | A separates content-grep, file-find, and tree; B merges into `localSearch`/`localFetch` |
| `local_find_files` | `localSearch` | merged |
| `local_view_structure` | `localSearch` / `localFetch` | merged; no standalone native "structure" tool |
| `local_fetch_content` | `localFetch` | 1:1 |
| `local_dead_code` | — (**A-only**; overlaps `lspSearch`/`astSearch`) | No dedicated native tool; capability not confirmed present in B |
| `lsp` | `lspSearch` | 1:1 |
| `package_search` | `artifactSearch` | 1:1 (rename) |
| — | `astSearch`, `astRewrite` | **B-only** — no granular AST tool exists in A on `main` |
| — | `jevReasoning` | **B-only** — reasoning tool, no TS analogue |
| — | `ghGetHistoryItem` | **B-only granularity** — A has no separate "fetch one history item" tool |

Net asymmetries to resolve before any merge: A-only `github_search_discussions` and `local_dead_code`; B-only `astSearch`/`astRewrite`/`jevReasoning`/`ghGetHistoryItem`. The `github_search_*` → `ghSearch{operation}` and `local_*` → `localSearch/localFetch` collapses are the bulk of the taxonomy delta.

## 3. The two-schema-authority problem

Neither side is fully self-authored, but they draw their schemas from **different** sources, and only *part* of A is bound to the shared source.

**A (tools-core) — partial-core, per-tool hand-authored schema:**
- Tool **names, descriptions, systemPrompt, baseSchema** come from `@octocodeai/octocode-core` via `completeMetadata`: `src/tools/toolNames.ts` (`STATIC_TOOL_NAMES = completeMetadata.toolNames`), `toolMetadata/descriptions.ts`, `toolMetadata/baseSchema.ts` (`completeMetadata.baseSchema`), and the server prompt in `octocode-mcp/src/index.ts` (`instructions: completeMetadata.systemPrompt`).
- But each tool's **validation schema is a hand-authored `scheme.ts`** — e.g. `src/tools/github_search_code/{scheme.ts,execution.ts,finalizer.ts}` and `src/tools/github_search_pull_requests/scheme.ts`.
- Those `scheme.ts` files pull **numeric limits from a local constants file** `src/tools/../config.ts`, not from core. Example: `github_search_pull_requests/scheme.ts` does `.default(GITHUB_SEARCH_DEFAULT_LIMIT)`, importing from `config.ts`.
- `config.ts` (`main`) hard-codes the limits: `GITHUB_SEARCH_DEFAULT_LIMIT = 30`, `LOCAL_MAX_DEPTH = 20`, `MAX_CHAR_LENGTH = 50_000` (also `GITHUB_SEARCH_MAX_LIMIT=100`, `MAX_CONTEXT_LINES=100`, `LOCAL_MAX_FILES_PER_PAGE=50`, …).

**B (native) — single fingerprinted authority generated from core:**
- The contract is **generated** into `crates/runtime/src/contracts/generated/` (`contracts.rs`, `tool-contract.json`, `contract-fixtures.json`) and exposed via `contracts::CONTRACT_JSON` / `contracts::contract_fingerprint()` (`contracts/mod.rs`).
- Provenance is pinned: `generated/contract-provenance.json` → `sourcePackage:"@octocodeai/octocode-core"`, `sourceRevision:"5dea8fc…"`, `sourceDirty:false`, `contractFingerprint:"fbd97624…"`; `generated/contracts.rs` sets `CONTRACT_FINGERPRINT = "fbd97624…"`.
- Two guards enforce single-authority: `generated_contract_has_clean_matching_provenance` (provenance fingerprint == `contract_fingerprint()`) and `no_inline_schema_literals_outside_generated_contracts` (fails the build if any `.rs` outside `contracts/generated/` hand-authors `"$schema"`/`"inputSchema"` — "move them to octocode-core and regenerate", `contracts/mod.rs:346-383`).
- The same numeric limits are baked into that generated contract and surface in the live MCP schema: `responseCharLength` `maximum: 50000` and tree `maxDepth` `maximum: 20` — i.e. the native mirror of `MAX_CHAR_LENGTH` and `LOCAL_MAX_DEPTH`.

**Where they silently drift.** Core generates B's contract and is fingerprint-guarded; core does **not** generate A's `config.ts` constants, and A's per-tool `scheme.ts` limits are guarded for **descriptions** only — `tests/tools/metadataProvenance.test.ts` asserts descriptions are byte-identical to core, but there is **no equivalent guard for the numeric limits**. So:
- A limit change in core → B regenerates, fingerprint changes, provenance test enforces it. A's `config.ts` stays at the old literal → the two servers now validate the *same tool* against *different bounds*, with no failing test.
- Today `MAX_CHAR_LENGTH=50_000`/`LOCAL_MAX_DEPTH=20`/`GITHUB_SEARCH_DEFAULT_LIMIT=30` (TS) coincide with the native contract's `50000`/`20`/default `30`. That coincidence is maintained by hand.

## 4. Config-flow differences

- **A — module-level mutable singleton.** `src/serverConfig.ts` holds `let config: ServerConfig | null = null` plus a `let initializationPromise`. `initialize()` mutates the module global from `getConfigSync()` (`@octocodeai/config`); `getServerConfig()` throws if `config` is still `null`; `cleanup()` resets both to `null`. State is process-global, order-dependent (must `initialize()` first), and mutable at any time.
- **B — runtime-owned immutable value.** `crates/runtime/src/config/resolver.rs::resolve_config(&ConfigInput) -> ConfigOutput` returns an owned `ConfigOutput { resolved: ResolvedConfig, …, revision: u64 }` (`config/types.rs:243`). It is passed by ownership/`Arc` (e.g. `providers/github/auth.rs`: `config: std::sync::Arc<ConfigOutput>`), carries a monotonic `revision`, and is never a mutable global. Limits like `PaginationConfig.defaultCharLength` live inside the resolved value; schema *bounds* live in the generated contract.

Consequence: A's config identity is "whatever the singleton currently holds"; B's is "this immutable `ConfigOutput` at revision N". Converging behavior requires reconciling both the *values* and the *ownership model*, but the values are the drift risk; ownership is a stylistic gap.

## 5. Options

### Option 1 — `octocode-core` as the sole taxonomy + limits authority feeding both (recommended)
Move the limit constants (`GITHUB_SEARCH_DEFAULT_LIMIT`, `LOCAL_MAX_DEPTH`, `MAX_CHAR_LENGTH`, …) out of `tools-core/src/config.ts` into `octocode-core` alongside `completeMetadata`, and have `scheme.ts` read them from core (as names/descriptions already are). B already regenerates from core and is fingerprint-guarded, so this makes core the one authority for **both**.
- Pros: eliminates the drift class entirely; reuses the pattern already proven for descriptions; the native guards keep B honest, and a mirror test keeps A honest.
- Cons: touches every `scheme.ts` that imports `config.ts`; requires core to own the limit vocabulary and a codegen/publish step for the TS side.

### Option 2 — name-mapping manifest + limits parity test (bridge, not merge)
Keep both surfaces but add (a) a checked-in `A↔B` name/`operation` manifest in core, and (b) a parity test asserting each shared limit in `config.ts` equals the corresponding bound in the generated native contract (extend `metadataProvenance.test.ts`-style checks to numbers).
- Pros: cheap; no schema rewrite; makes drift a red build instead of a silent runtime divergence; documents the taxonomy collapse for humans and tooling.
- Cons: does not remove the duplication, only detects it; the manifest itself must be maintained.

### Option 3 — status quo + deprecation path to B
Freeze A's taxonomy, treat B as forward surface, and deprecate granular A tools as native equivalents reach parity (closing `github_search_discussions`, `local_dead_code`; exposing `astSearch`/`astRewrite`).
- Pros: no reconciliation work now; aligns with the in-flight Rust migration.
- Cons: leaves the silent-drift window open for the entire deprecation period; parity gaps (discussions, dead-code) block A's retirement.

## 6. Recommendation & smallest first step

Adopt **Option 1** as the target (core owns taxonomy + limits for both), and land **Option 2's parity test first** as the smallest, reversible step:

> **First step:** add one test in `octocode-tools-core/tests` that asserts `MAX_CHAR_LENGTH`, `LOCAL_MAX_DEPTH`, and `GITHUB_SEARCH_DEFAULT_LIMIT` from `config.ts` equal the corresponding values in the native generated contract (`contracts/generated/tool-contract.json` bounds: `responseCharLength.maximum`, tree `maxDepth.maximum`, PR/repo search default). This converts today's hand-maintained coincidence into a build gate with zero schema changes, and it's the acceptance check for the later core-authority move.

Then migrate the constants into core and rewire `scheme.ts` imports (Option 1), retiring the parity test once both sides read the same source.

## 7. Risks & non-goals

**Risks**
- Moving limits to core is a cross-package refactor; a missed `scheme.ts` import would relax a bound silently — the parity test (step 1) must exist first to catch it.
- `github_search_discussions` and `local_dead_code` capability parity in B is **unverified**; confirm before treating A tools as retireable.
- B's `no_inline_schema_literals_outside_generated_contracts` guard means any TS-driven contract change must go through core codegen, not a Rust edit — plan the workflow accordingly.

**Non-goals**
- Not unifying the two runtimes or the config *ownership* model (mutable singleton vs `Arc<ConfigOutput>`); this RFC targets the schema/limits authority, not process architecture.
- Not adding `jevReasoning`/`astRewrite` to A, nor removing any A tool in this RFC.
- Not changing any limit's *value*; only its *source of truth*.

*Evidence: files read on `main` via ghGetFileContent/ghSearch (`packages/octocode-tools-core/src/tools/*`, `src/config.ts`, `src/serverConfig.ts`, `src/tools/toolNames.ts`, `toolMetadata/{descriptions,baseSchema}.ts`, `octocode-mcp/src/index.ts`) and local reads under `packages/octocode-native/crates/runtime/src/` (`contracts/mod.rs`, `contracts/generated/{contracts.rs,contract-provenance.json,tool-contract.json}`, `config/{types.rs,resolver.rs}`, `providers/github/auth.rs`) plus the live native MCP tool schema (`responseCharLength.maximum=50000`, tree `maxDepth.maximum=20`).*
