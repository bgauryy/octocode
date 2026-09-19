# Rust Migration — Cleanup & Hardening Improvements

**Status:** proposed · **Date:** 2026-09-19 · **Branch context:** `codex/preproduction-hardening`
**Method:** evidence gathered with octocode local/engine reads; each item carries a jev-style grounded verdict (claim → evidence → verdict) with `file:line` citations.

## Context

The Rust migration moved tool execution, config resolution, and the search/GitHub
engine into `packages/octocode-native` (crates `engine` + `runtime`). The Node CLI
(`packages/octocode`) and MCP server (`packages/octocode-mcp`) are now **thin
delegators** and were correctly emptied of their old flow:

- CLI: `cli/index.ts:48` routes to the native binary via `delegateToNative`; only
  `skill` + interactive install stay Node-owned (`native-delegate.ts:12`).
- MCP: 12-line `index.ts` → `native/index.mjs` builds a `NativeRuntime`, registers
  tools from the catalog, executes via `runtime.executeMcp` (`native/index.mjs:68`).
  No `tools/`, `handlers/`, or engine JS remains.

What the migration did **not** finish is **configuration** and **tool-metadata**
single-sourcing, plus a staleness/growth gap in the engine's GitHub disk cache.
Five improvements follow, ordered by leverage.

---

## 1. Collapse JS config resolution into the native resolver

**Claim.** The full JS config-resolution pipeline is dead weight relative to the
runtime and now only duplicates the Rust resolver against the same `.octocoderc`.

**Evidence.**
- Rust owns runtime config: `crates/runtime/src/config/*.rs` (~1,736 lines),
  constructed per request and used by CLI + MCP.
- JS package `@octocodeai/config` (~1,764 lines) still ships a full
  resolver/validator/loader, but its resolution APIs have **zero** runtime
  consumers: `resolveConfig`, `resolveConfigSync`, `validateConfig`,
  `getDefaultConfig`, `resolveTokens`, `resolveGitHubToken` are imported nowhere in
  CLI/MCP/native-js source. The CLI imports the package for exactly one symbol —
  `setRuntimeSurface` (`packages/octocode/src/cli/index.ts:1`).
- What keeps the package alive is host-only plumbing: `getOctocodeHome`,
  `isPersistentStorageEnabledForExtension`, `propagateOctocodeEnv`. But
  `isPersistentStorageEnabledForExtension` → `getConfigSync` still drags in the
  heavy pipeline: `config/validator.ts` (455) + `config/resolverSections.ts` (237)
  + `config/resolverCache.ts` (127) + `config/defaults.ts` (90) ≈ **~900 lines**
  that re-implement what Rust already resolves.

**Why it matters.** Two independent resolvers of one file guarantees eventual
divergence in defaults, validation, and precedence (item #3 is a live instance).
Every future config key must be added in two languages or silently disagree.

**Recommendation.** Reduce the Node host to a ~50-line reader (or a small native
call) that answers only what it needs — home dir, `storage.mode`, env propagation —
and delete `validator.ts` / `resolverSections.ts` / `resolverCache.ts` /
`defaults.ts`. If a native call is preferred, the runtime already computes
`is_persistent_storage_enabled(&config.resolved)` (`runtime/engine.rs:173`); expose
that + resolved home over the existing binding instead of re-resolving in JS.

**Verdict.** GROUNDED — safe to retire; no runtime path reads the JS resolver.
Scope the Pi-extension's exact needs first (see Sequencing).

---

## 2. De-vendor `octocode-config.mjs` (16 copies)

**Claim.** A third config implementation is copy-pasted per skill and can drift
independently of both Rust and the JS package.

**Evidence.**
- `octocode-config.mjs` is a self-contained **977-line** bundle (defines its own
  `getConfigSync` / `getConfigValue` / `loadOctocoderc` / `PROTECTED_KEYS`; it does
  **not** import `@octocodeai/config`).
- It exists in **16 locations**: 4 skill sources under
  `packages/octocode/skills/*/scripts/` and 12 more under
  `packages/octocode-pi-extension/dist/skills/*/scripts/`.
- These copies back the skill runtime (jev, brainstorming, chrome-devtools,
  scraping): `getConfigSync`, `getConfigValue`, `getRuntimeSurface`,
  `propagateOctocodeEnv`, `validateConfig`, `loadOctocoderc` all resolve to the
  vendored file, not the package.

**Why it matters.** 16 hand-synchronized copies means a security or precedence fix
lands in one and not the rest. Item #3's protected-key gap already lives in this
file.

**Recommendation.** Generate the vendored `.mjs` from a single source at build time
(the skill build already bundles), or import the built package. Either way, one
source of truth so `PROTECTED_KEYS` and validation cannot drift per-skill.

**Verdict.** GROUNDED — 16 identical 977-line copies confirmed by content hash of
line count; consolidation removes a standing drift surface.

---

## 3. Fix `PROTECTED_KEYS` drift — Rust protects 2 keys the JS/mjs sets do not

**Claim.** The JS and vendored-mjs `PROTECTED_KEYS` are missing two entries the Rust
runtime protects, and both are security-relevant.

**Evidence.**
- Rust: `crates/runtime/src/config/types.rs:15` — `PROTECTED_KEYS: [&str; 19]`.
- JS: `packages/octocode-config/src/index.ts` — 17 keys.
- mjs: `packages/octocode/skills/*/scripts/octocode-config.mjs` — same 17 keys.
- Exact diff — present in Rust, **absent** in JS/mjs:
  - `GH_HOST` — selects the GitHub host (gh-CLI convention). Rust protects it "so an
    untrusted `.env` cannot redirect API traffic to an attacker-controlled host."
  - `OCTOCODE_ALLOW_PRIVATE_REGISTRY` — SSRF opt-in for private/loopback/link-local
    registries. Rust protects it "so an untrusted `.env` cannot flip it on."

**Why it matters.** In any JS/mjs config path (skills, Pi extension) a repo-local
`.env` could set `GH_HOST` (exfiltrate tokens to an attacker host) or
`OCTOCODE_ALLOW_PRIVATE_REGISTRY` (enable SSRF) — the two protections that exist in
Rust do not apply there. This is a concrete instance of #1/#2 already diverging.

**Recommendation.** Immediate: add both keys to the JS set and the vendored mjs
copies. Durable: fold into #1/#2 so the list has one owner. Add a test asserting the
JS/mjs protected set equals the Rust `PROTECTED_KEYS` (parity test).

**Verdict.** GROUNDED — exact 19-vs-17 diff verified against source; the two missing
keys are the security-sensitive ones. Fix now, single-source later.

---

## 4. Single-source tool metadata (`DIRECT_TOOL_DEFINITIONS` ↔ `tool-contract.json`)

**Claim.** MCP tool registration reads schemas from one source and
availability/instructions from another; both must be hand-kept in sync.

**Evidence.**
- Schemas (title, description, inputSchema, outputSchema, annotations) come from the
  external `@octocodeai/octocode-core/schema` `DIRECT_TOOL_DEFINITIONS`
  (`native/index.mjs:6,44`).
- Availability + `mcpInstructions` come from the native catalog
  (`runtime.catalog()`, `native/index.mjs:27,33,41`).
- Drift is a hard failure at startup: a native catalog tool with no matching
  definition throws `Native catalog tool has no contract: <name>`
  (`native/index.mjs:50`). Parity is currently guarded only by
  `octocode-mcp/tests/packageRelease.test.ts`.
- The native side already owns a generated contract:
  `crates/runtime/src/contracts/generated/tool-contract.json` (keys
  `contractFormatVersion`, `fingerprint`, `mcpInstructions`, `tools`).

**Why it matters.** Two sources of truth for the same tools means every add/rename/
schema change is a two-place edit whose only backstop is a release test; a miss
bricks MCP startup.

**Recommendation.** Generate `DIRECT_TOOL_DEFINITIONS` from the native
`tool-contract.json` (the contract already carries a `fingerprint` for change
detection), or vice-versa, so schema + availability + instructions derive from one
artifact.

**Verdict.** GROUNDED — dual source confirmed; failure mode is startup-fatal.
Lower urgency than #1–#3 because the release test currently catches drift.

---

## 5. GitHub engine cache — disk layer ignores TTL/revalidation and is unbounded

**Claim.** The in-memory GitHub cache is well-designed, but the optional disk tier
serves entries with no TTL, no revalidation for git-trees, and no size bound.

**Evidence.**
- Core cache `BoundedCache<V>` (`crates/runtime/src/cache/mod.rs`) is solid: LRU +
  entry/byte budgets, 300s TTL, config-revision invalidation, credential+endpoint
  partitioning, snapshot generations. Defaults: 1,000 entries / 32 MiB / 300s
  (`cache/mod.rs:31`).
- Disk tier (`runtime/github_cache.rs`) is enabled when persistent storage is on,
  rooted at `octocode_home/tmp/response` (`runtime/engine.rs:170-175`).
- **Gap A — disk reads bypass freshness.** On a memory miss/expiry the cache falls
  back to `read_disk`, which just `serde_json::from_slice`s the file
  (`github_cache.rs:53-57`) — no `expires_at`, no `revision` check. The 300s TTL and
  config-revision invalidation apply to memory only.
- **Gap B — git-trees are served stale.** `ghSearch` tree traversal stores the tree
  with `etag: None` (`tools/gh_search/tree.rs:338`) and, on hit, returns the cached
  `TreeResponse` directly with no network revalidation (`tree.rs:309-313`). Combined
  with Gap A, a disk-persisted tree for a moving branch head can be served
  indefinitely (until a config-revision bump or `clear()`), so newly-added files go
  unseen.
- **Gap C — disk is unbounded.** `write_disk` writes one file per resource hash with
  no eviction and no byte/entry budget (`github_cache.rs:59-66`); the `BoundedCache`
  budget governs memory only. `octocode_home/tmp/response` grows without bound; only
  `clear()` (invalidate_all) reclaims it.
- **Mitigated for file content.** `ghGetFileContent` always issues a request and
  uses the cache only as an ETag store — a 304 confirms freshness, otherwise it
  refetches (`providers/github/content.rs:109-153`). So Gaps A/B do **not** cause
  stale file bytes; they bite git-tree/`ghSearch` results specifically.

**Why it matters.** Persistent-storage users can get stale `ghSearch` results and an
ever-growing `tmp/response` directory. The correctness risk is scoped to git-trees
(no ETag), which is exactly the path with no revalidation.

**Recommendation.**
1. Persist `expires_at`/`revision` (or an mtime) with each disk entry and honor them
   in `read_disk` — drop or ignore expired/revision-mismatched files.
2. Give git-tree entries an ETag and send `If-None-Match` on tree fetches (the
   transport already supports conditional GET for content), so a hit revalidates
   instead of serving blind.
3. Bound the disk tier — cap entries/bytes with LRU by mtime, or reuse the
   `BoundedCache` budget semantics for disk.
4. Consider sourcing the cache `ttl`/budgets from resolved config rather than
   `CacheConfig::default()` so operators can tune them.

**Verdict.** GROUNDED — memory tier is strong; the three disk gaps are real but
correctness-bounded to git-tree/`ghSearch` (file content is protected by mandatory
ETag revalidation). Prioritize Gap A+B (staleness) over Gap C (growth).

---

## Suggested sequencing

1. **#3 now** (2 protected keys) — smallest change, closes a security gap in every
   JS/mjs path. Add a Rust↔JS parity test.
2. **#5 Gap A+B** — disk freshness + git-tree revalidation; scoped, correctness-
   affecting for persistent-storage users.
3. **#1 + #2 together** — single-source config: reduce Node host to a thin reader,
   generate the vendored mjs from one source. #3 then becomes structurally
   impossible to reintroduce.
4. **#5 Gap C** — bound the disk tier; fold cache config into resolved config.
5. **#4** — single-source tool metadata from `tool-contract.json`; lower urgency
   (release test currently guards it).

## Verification per item

- #1/#2: after consolidation, `grep` for the retired symbols returns only the single
  source; skills still run (jev/brainstorming/scraping smoke).
- #3: parity test asserts JS/mjs set == Rust `PROTECTED_KEYS`; attempt to set
  `GH_HOST` via a repo `.env` in a JS path is rejected.
- #5: with persistent storage on, mutate a branch, confirm `ghSearch` reflects the
  new file (no stale tree); confirm `tmp/response` stays bounded under load.

## Evidence appendix (paths)

- CLI delegation: `packages/octocode/src/cli/index.ts:48`,
  `packages/octocode/src/cli/native-delegate.ts:12`
- MCP wrapper: `packages/octocode-mcp/src/index.ts`,
  `packages/octocode-mcp/src/native/index.mjs:6,27,44,50,68`
- Rust config: `packages/octocode-native/crates/runtime/src/config/types.rs:15`,
  `.../config/resolver.rs`, `.../runtime/engine.rs:170-175`
- JS config: `packages/octocode-config/src/index.ts`,
  `.../src/config/{validator,resolverSections,resolverCache,defaults}.ts`
- Vendored mjs: `packages/octocode/skills/*/scripts/octocode-config.mjs`
- Engine cache: `packages/octocode-native/crates/runtime/src/cache/mod.rs`,
  `.../runtime/github_cache.rs`, `.../providers/github/content.rs:109-153`,
  `.../tools/gh_search/tree.rs:308-345`
