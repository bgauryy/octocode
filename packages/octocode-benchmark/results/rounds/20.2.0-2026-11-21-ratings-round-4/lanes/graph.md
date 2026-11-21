# Rate round 4: Graph lane (graphBuild, graphQuery)

Build: `octocode 20.2.0+56f67471` (`node packages/octocode/out/octocode.js --version`); repo HEAD e9309aa, tree dirty (113 status lines; other agents editing chrome-devtools during the lane). All calls via CLI `--json` with `OCTOCODE_GRAPH_HOME=.octocode/tmp/rate4-graph/home` (deleted after the lane).

Scopes: fixture (15 graph files, TS/Rust/Python, known edges), `crates/engine` (687 files), `packages/octocode-chrome-devtools` (249→255 files), `packages/octocode-native` (3,554 files, stress).
Spend: ~37 calls (18 logged + ~19 page walks), ~350 K chars.

## Table

| tool | Desc | Schema | Workflow | Clarity | Time | Tokens | Paging | Smart | Short | Long | Stress | Hints | Overall | Correct |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| graphBuild | 8 | 9 | 9 | 8 | 8 | 9 | – | 8 | 9 | 8 | 8 | 9 | **8.5** | 9 |
| graphQuery | 8 | 8 | 8 | 8 | 9 | 7 | 9 | 8 | 8 | 8 | 8 | 9 | **8** | 7 |

- graphBuild: in: structureSearch → graphBuild → out: graphQuery stats/diagnostics (related)
- graphQuery: in: graphBuild → graphQuery → out: lspSearch callers/references (related), graphBuild (stale/reingest)

Timings: fixture cold build 0.59 s; engine 8.1 s; chrome-devtools 2.3 s; native 3,554 files 51 s (machine load ~3.4/CPU); warm unchanged build 0.17 s (`reused`). Queries 0.09–1.5 s, 5-query batch on native 1.47 s.

## Verified correct (ground truth)

- Fixture callers of `rs/src/num.rs#sat`: u1 (import), u2 (module-path), u3 (namespace), deepf (module-path) = all 4 call sites by reading the files.
- Fixture cycle d.ts<->e.ts, unreachable orphan.ts, unused exports applyAll/Square (unusedB skipped because b.ts is namespace-imported, stated in controls), path main→run→e1→d2.
- Engine `impact src/num.rs#saturating_u32 kind:symbol`: byDepth.1 = 42 at depth 1 and depth 2 (identical 21-row first-page sets); walking both pages = 43 rows; vs `rg -l saturating_u32` (42): only diffs are lib.rs (`pub mod num;`, correct) and tests/local_bindings.rs (declares its own fn, correct to omit).
- chrome-devtools `impact since:HEAD`: 84 changed graph files + 15 unmapped (.md) = 99 = `git status --short` count; depth-0 set equals git set exactly; 114 rows over 2 pages, no dups.
- hubs fanIn 45 for cdp-connection.mts = rg import count 45 (prod). cochange cli.mts: 9 commits, open-browser 9, cdp-runner 8, OPERATING.md 6 = git log; `linked:false` correct (string-launched).
- Native-scope depth-1 impact byDepth.1 = 103 at depth 1 and 3.
- Issues mentionedIn on a fresh snapshot: 575/575 cited lines contain the subject.
- Errors: unknown target, unknown snapshot id, field not allowed for form, no snapshot for path, outside-root path — all typed (`notFound`/`invalidInput`/`outsideAllowedRoots`) with a runnable related/fix.

## Defects (most severe first)

1. **R4-G1 med, native — symbol impact mixes file-level false positives and silently misses a real call.** Input `{"operation":"impact","path":"packages/octocode-native","target":"crates/engine/src/num.rs#saturating_u32","kind":"symbol","depth":1}` (3 pages, 104 rows). Depth-1 lists `runtime-base/src/response/pages/publish.rs`, `runtime/src/runtime/row_indices.rs`, `tool-github/src/gh_structure/materialize.rs`, `tools-shared/src/source_view/block/recovery.rs` via `imports:rust-path`; rg shows each uses only `octocode_engine::num::saturating_usize` (e.g. publish.rs:220), never saturating_u32. Meanwhile `tool-local/src/structure_search/listing.rs:176` calls `octocode_engine::num::saturating_u32(offset.get())` inside a `macro_rules! listing_fields` body (listing.rs:111) and is absent from impact, callers (`edges:[calls,passes]`, `confidence:low`) and `unresolved symbolName:saturating_u32`; yet `deps listing.rs` shows num.rs. Expected: symbol precision excludes sibling-symbol users (or labels them file-level), and macro-body sites are disclosed (unresolved reason or coverage note).
2. **R4-G2 low-med, native — `testsToRun.functions` names module-level helpers, not test functions.** `next.testsToRun` of `impact since:HEAD` on chrome-devtools: `tests/unit/attach-gate.test.mjs` functions = FAKE, OPEN, chrome, foreign, lastLine, open, root, skill, stop, tabsOf; source: lines 11-33 are consts/helpers, the tests are anonymous `test('…', async () => …)` at 46/68/128. Output doc: "functions names the test functions reached". summary.testFunctions 177 is inflated accordingly.
3. **R4-G3 low, native — Rust call through a `#[path]` inline module stays unresolved with a wrong reason.** Fixture `rs/src/lib.rs:9` `inl::deep::deepf()` (module `#[path="extra"] pub mod inl { pub mod deep; }`): `unresolved` reason `member-unknown-receiver-in-repo`; `callers rs/src/extra/deep.rs#deepf` → 0 rows (warning discloses 1 unresolved site). File link now works (`dependents deep.rs` → lib.rs `imports:rust-path`).
4. **R4-G4 low, native — `sites` undercounts when one caller reaches a callee by two bindings.** Fixture `callers ts/src/b.ts#helper`: alpha row `sites:1, via:import`; `witnesses to:helper` shows two call sites at a.ts:3 (`helper(x)` import, `B.helper(1)` namespace). Expected sites 2.
5. **R4-G5 low, native — issues on a stale snapshot cite lines from files added after the build.** Stale chrome-devtools snapshot: 11/592 mentionedIn lines did not contain the subject, all in files `stale` reports as `added` (console-log-check.mts:6 is `import {`, the path is on line 11). The page carried `freshness:{fresh:false}`, so it is disclosed; after rebuild 0/575 mismatches.
6. **R4-G6 low, native/core — impact rows shape differs by depth.** Fixture depth-1 rows omit `from`; depth-2 rows include it. Minor.

## Regression check (round 3)

| id | status | evidence |
|---|---|---|
| R3-G1 impact depth-1 varies with depth | **fixed** | engine byDepth.1 42 at depth 1 and 2; native 103 at 1 and 3; set = rg minus correct exclusions |
| R3-G2 since:HEAD over-seeding from package.json edit | **not retestable as stated** (fixture git commits refused by the workspace guard; no package.json edit in the tree); analogue fine | 99-file working-tree change seeded exactly 84+15 unmapped; `configChanges` field present |
| R3-G3 #[path] inline module unlinked, tier-90 unreachable | **partly fixed** | file now linked and not reported unreachable; the call `inl::deep::deepf()` still unresolved (R4-G3) |
| R3-G4 impact on a directory notFound | **fixed** | `impact target:"src/engine/cdp-checks"` → 64 changed + 11 depth-1 rows |
| R3-G5 mentionedIn line lacks the path | **fixed** (fresh snapshot) | 575/575 lines contain the subject; stale-only variant R4-G5 |
| R3-G6 testsToRun rows without functions undocumented | **fixed** (doc) | output schema: a row without functions … "run the whole file"; 14/46 and 7/7 rows lack functions; new content issue R4-G2 |

## Unverified

- native filesScanned 3,554 vs `rg --files` 3,681 minus listed no-grammar types and 7 skipped ≈ 3,564: ~10 files (likely binaries/extensionless such as fuzz seeds, .pom, .jpg) not itemised; not checked whether they are disclosed.
- drift rows (added/changed) matched `stale` rows for the same snapshots; not checked beyond that.

## Good

- Every refusal is typed with a runnable fix; depth-limited impact always says how many files were left out and gives the exact deeper call; stale snapshots announce themselves.
- Warm rebuild is instant and page walks rebuild rows exactly once (no duplicates across 2–5 page walks).
