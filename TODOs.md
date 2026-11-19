# TODOs (2026-11-18, updated 2026-11-19 night 2)

This lists every open item from round 3 (tool-ratings fixes) and the velocity work, with what the evening session did about each. The owner asked for this file; delete it when the owner says so.

**State of the tree:**
- Nothing is committed.
- After the end-to-end RFC run (2026-11-19 day): `verify:own` unit run 21 binaries, 9,034 passed, 0 failed; process run 465 of 465. `lint:rust` clippy clean; `docs:verify` passed. `build:dev` staged at 13:41 with the clasify batching (TD1) and cleanup (TG1) changes. See "End-to-end RFC run" below.
- Earlier, after the swarm fixes: `verify:own` unit run 21 binaries, 9,022 passed, 0 failed; process run 466 of 467 (W16, a known load flake). `dev.mjs lint:rust`: clippy clean. `docs:verify` passed. `build:dev` (mac only) staged `npm/darwin-arm64/octocode`. Live check: securityCheck event-stream 3.3.6 says `affected:true` (GHSA-mh6f-8j2x-4483); `^4.17.0` warns it was checked at 4.18.1 only
- Contract regenerated from core (`importerScan` enum); `yarn.lock` changes only the core content hash. `Cargo.lock` lost the two mimalloc crates, nothing else.

**Plans:**
- The owner deleted `.octocode/` (2026-11-19, on purpose), which held the RFCs. The end-to-end RFC survives only in the session scratchpad (`rfcw/new.md`); the velocity RFC is gone.
- Round 3: `packages/octocode-benchmark/campaigns/20.2.0-2026-11-16-round3/ROUND.md`.

## Owner decisions

| # | Decision | State |
|---|---|---|
| D1 | Review and commit the tree | open, owner (agents never commit) |
| D2 | Developer Tools for the terminal and VS Code: `sudo spctl developer-mode enable-terminal`, then System Settings → Privacy & Security → Developer Tools | open, owner |
| D3 | A releasing allocator for the warm daemon | closed for now: the unmeasured mimalloc feature was removed (nothing enabled it, and it broke the all-features gate). Research kept: The binary runs on xzone malloc (`_xzm_*` in a `sample`), whose `pressure_relief` is `return 0;` (libmalloc `malloc_common.c:161`); the old magazine zone's relief also returns 0 whatever it frees (uncounted racks, a shadowed `total`). mimalloc purges with `MADV_FREE_REUSABLE` (immediate footprint accounting) and has `mi_collect(true)`. Next: A/B system / mimalloc+collect / jemalloc on idle footprint, peak and instructions. The cheap `DYLD_INSERT_LIBRARIES` A/B was blocked by the session's auto-mode classifier |
| D4 | Keep `lsp.prewarm` default `targeted`? | recommend keep; no change made |
| D5 | lspSearch scans that walked gitignored trees | done: the ignored-file pass is gone, and the watched-file fingerprint now honors ignore rules, the dot rule and the default prune (it walked the server root lifted: 101k dirs / 808k files against 1,240 / 5,363 honored here). Only the build-script output rust-analyzer loads (`target/*/build/*/out`) is walked past the prune, with build scripts on |
| D6 (new) | Memory-pressure handling: drop the memos (budget: an eighth of RAM, 600 s idle) and purge the allocator on an OS pressure signal | open, design |

## P1: velocity

| # | Item | State |
|---|---|---|
| V1 | Heaviest test tails | partly done. The two `local_search::tests::streamed` tests (each over 60 s) now sweep only the hit counts near each page cut, found by one probe walk, and assert the exact fill they exist for; the other stops its first walk after 3 pages. tool-local's binary: 226 s → 142.5 s. Open: `pages::large_scan`, tool-graph (100 s), and the process run, now the gate's longest leg (182.9 s against the unit run's 173.8 s); its four longest tests now start first (`.config/nextest.toml` priority), not yet measured |
| V2 | libtest tests that start child processes | done. `PROCESS_TESTS` now holds every test whose children outlive a quick exit or whose result waits on a child: the LSP client fakes, runtime LSP and channel tests, process supervision, serve fakes, probes and credential helpers, the live checks and the stand-in browser. The cause (macOS marks pipes and sockets close-on-exec only after creating them, so a concurrent spawn inherits them) is written beside the list |
| V3 | `test:dev --crate X --lib` still rebuilds the facade and CLI | open, decision: cargo-hakari needs a `cargo install`, or the nightly `-Zfeature-unification` |
| V4 | `#[path]` mounts of shared fixtures | open. Plan: runtime-base compiles `test_support/checks.rs` once more as a published-stage module of its own (the stage functions differ), and a dev-only crate holds `runtime/tests/support` for runtime tests, benches and cli tests (`Workspace::cli` becomes an extension trait). `octocode-test-support` cannot hold the runtime fixture: it would depend on the facade |
| V5 | An engine edit rebuilds 14 crates | measured ceiling: the engine's own compile is about 50–75 s of the 115–129 s; runtime-base (`security` 64 uses, `search` 18) and tools-shared (every domain) reach into the engine, so a domain split still rebuilds the same dependents. Split not started |
| V6 (new) | Runner fixes found this session | done: the build of `test:rust`/`test:dev` keeps the caller's TMPDIR (a compile started an sccache server inside the private test root, and every compile failed once the root was removed); `test:dev` no longer overwrites the full run's `target/.keep/test-durations.json`. Both with failing-first script tests |

## P2: round-3 leftovers

| # | Item | State |
|---|---|---|
| R1 | SMALL-4 | done: hover text judged with its markdown escapes undone (`judge_unescaped`), the premise asserted; ghSearchCode's count-change warning names both counts, why and the re-walk |
| R2 | LOCAL-Z | done except the regex shadow matcher (deferred: it would put a full regex pass on every multi-term literal search to save a second walk only when a term is dead). Each term now counts its own matches (`rg -o -F` per term), which also retires the overlap re-probe in the dead-term reading; a check's binary files are counted once, inside its clause; `ignored-default-excluded` entries keep the default rule as their reason |
| R3 | CLEAN-1, ENGINE P2, HEAVY-2, CLEAN-2 | CLEAN-1 done: `commit` → `commitSha` in two fixtures plus a test of the repair; one `BRIEF_FIELDS` (contracts layer); the `error_hint` doc; write-only `star_reexporters` removed (resolution reads the edges); 50 inline test modules moved to their own files (one path-mounted build file kept inline). ENGINE P2, HEAVY-2 (95 files against an old base, `.octocode/build-audit/2026-11-17/fix-all/HEAVY-2/logs/`) and CLEAN-2 not started |
| R4 | Stale-socket flake | done: the test's stale socket is bound and closed and never listens, so an inherited copy cannot answer. The product cannot hit this: `serve` claims its socket before it starts any runtime or child |
| R5 | drift on an unrebuilt graph | done: an empty answer from a stale snapshot hints to rebuild and leads to the graphBuild |
| R6 | PPK MAC false positive | done: `Private-Lines:` and `Private-MAC:` mark a block edge only with their value |
| R7 | Load-sensitive engine test | done: the test pins its stream's share (`with_share`) at 1, 2, 4 and 12 |
| R8 | Core enum, Examples, rowPart, lodash | enum done (core + regen). rowPart rule was already in the instructions. Examples echoing every valid field: kept (copying the Example reruns the call as sent; a size rule would be a cap). lodash: an `advisory` lookup lists OSV's and GitHub's record of one GHSA as two entries, by the tested design that keeps each source's record whole (`record_projection` tests); merging or labelling them is an owner decision |
| R9 | Small instruction increases | open: needs a release pin of this tree and a re-sample |
| R10 | Re-rate the scorecard | done: a re-rating section on `bin-r4o` in the scorecard; five of six rows retested by hand (pkg-2 rests on round 3's check). ghSearchCode 4.0 → 8.4, securityCheck 4.0 → 8.4, lspSearch 6.7 → 7.5, graphQuery 8.1 → 8.7, clasify 7.8 → 8.0 provisional |

## Housekeeping

| # | Item | State |
|---|---|---|
| H1 | `.octocode/` is 10 GB | open, optional |
| H2 | Two orphaned `prov.mjs` processes (PIDs 33383, 58352): another project's finished session; script deleted, parent launchd, 1.5 MB each, idle on a localhost port | open: stop by PID (`kill 33383 58352`); the session's auto-mode classifier refused |
| H3 | Pins `bin-r4l` and `bin-r4o` | open: `bin-r4o` is now the re-rating's evidence binary; delete when the owner says |
| H4 (new) | sccache server PID 63512 runs with TMPDIR `/tmp/octocode-rust-test-KmT7Z1` (V6's defect); the directory was recreated so builds work | remove the directory once that server exits (`sccache --stop-server` ends it) |

## Roast and simplify (night)

Done, each with tests:

| # | Change |
|---|---|
| S1 | localSearch: an empty result reads nothing more. Skipped paths are named and `includeSkipped` lifts their causes; the case-insensitive and regex readings, and the parent-folder search, are offered from the request alone, as ripgrep leaves `-u` and `-i` to its user |
| S2 | structureSearch: an empty listing's three re-walks (lifted flags, dropped name filters, dropped depth bound) are now leads built from the query. `ProbeBound`, the `Walking` probe mode and its flags through the walk steps are gone |
| S3 | Engine listing walk: an ignored dot entry is counted as ignored. It was counted nowhere, which the probe hid |
| S4 | lspSearch fingerprint: see D5 |
| S5 | Test-code `#[path]` declarer walk (graphBuild, astSearch `testCode`) honors `.gitignore`; it was a raw `read_dir` (3,000 ignored `.rs` took graphBuild from 0.83 G to 3.9 G instructions) |
| S6 | astSearch's fuller-form note is a warning on a page with matches: success rows drop `hints`, so it was lost where it mattered |
| S7 | Test-only crates moved to dev-dependencies (runtime `toml`/`regex`/`base64`, runtime-base `futures-util`, tool-local `octocode-io`/`secrecy`/`tokio`, tool-clasify `octocode-github`/`tokio`, tool-github `libc`). tool-local no longer depends on io, nor tool-clasify on github |
| S8 | Dev profile builds tree-sitter-cuda (the largest parser) at opt-level 0, like cpp, c-sharp and scala |
| S9 | Removed: the mimalloc feature (D3), the `OCTO_D7_BEFORE` toggle in lspSearch, a stale module doc |

## Open items O1–O5 (night 2: "do all", profiled)

Every tool was profiled before and after on the mac dev build (`scratchpad/prof/run.py`: retired instructions of the measured call, min of up to 3 fresh servers, cwd `packages/octocode-native`). Table in "Profile" below.

Done, each with tests:

| # | Change |
|---|---|
| O1 | No local tool probes skipped paths any more. The structural walk (`engine/src/structural/files/walk.rs`) applies the ignore and dot rules itself, through the text search's `EntryRule`, plus the default prune, and records each skip (`WalkSkipped`). The listing walk (`fs_query`) now records dot entries too (`FileSystemQueryResult::hidden`). astSearch match, astSearch symbols and graphBuild feed those records into their `WalkAdmission` (`record_left_out`), the one `WalkSkips` ledger localSearch uses. An empty answer says `Skipped paths (N ignored, …) were not searched` and its `includeSkipped` lead lifts the recorded causes. A page with matches only counts them in `skippedPaths`. Deleted: `empty_walk` diagnosis, `FileProbe`, `ProbeWork`/`ProbeLimits`, reasons, `Verdict`, `leads::skipped::disclose`/`Unchecked` (1,004 lines of probe code and tests); graphBuild's extra `unparsed` walk is now the recording walk |
| O2 | astSearch's alternate checks count matches (`StructuralDemand::Count`) instead of pricing each as a JSON row. The construct check asks only whether any node of the kind exists, through the new `StructuralDemand::Exists`, which ends the scan at the first chunk holding a match |
| O3 | lspSearch: fingerprint walker threads keep their own lines and reads, merged once per thread; no lock is taken per entry, and none is held over ignore-file stats. The spelling-index tokenizer judges ASCII bytes without decoding, and the pass that builds a file's entry answers a miss from the new hashes. Watch registrations are kept by id, so one `unregisterCapability` ends only its own watch. The watched snapshot is keyed by absolute path, and deletions are judged only inside the roots the walk covered: a request from another root no longer reports every file created |
| O5 | One engine-error mapping (`ToolError::engine`) for localSearch, structureSearch and astSearch. localSearch had dropped `timeout`, `cancelled`, `permissionDenied`, `invalidPattern` and `notFound` to `executionFailed`. lspSearch's project include check now uses the shared glob dialect and word search; it matched globs against bare file names. Removed: the dead `size → sizeBytes` spelling row, and tool-lsp's `globset` dependency |

Open, measured, not changed:

| # | Item | Evidence and next step |
|---|---|---|
| O3a | Rust references text-scan the whole git root | Narrowing the scan to the crate is unsafe as is: `ItemReach::reaches` counts `#[path]` modules and `include!` files outside the crate directory, and finding them reads the whole package. Next: list those targets once per package and scan them beside the crate directory |
| O3b | Coverage runs on the async worker | Under 5.8% of the repeat profile (not in the top 40 frames); left synchronous. `block_in_place` would panic on the single-thread test runtime |
| O3c | The cold call builds no spelling index | Call 1 uses the engine text scan; call 2 builds the index (20.1 G before, 16.3 G after); calls 3+ cost 2.3 G. Next: build the entries during the cold scan's own read |
| O4 | graphBuild after a one-file edit | Disk storage: 60.0 G against 91.6 G cold. Of the rebuild: decoding cached facts 22% and unpacking them 16%, re-projecting the graph 24%, publishing 15%, and `keep_file` 17%, which re-runs the secret scan on every reused file. Next: an in-process memo of decoded artifacts by content digest; reusing judged sources needs a security review first. Memory storage reuses nothing (70.8 G of 71.7 G) |
| O4b | graphQuery decodes per call | Load 23% (`decode_tables` 21%), staleness 15%, `is_sensitive_path` 7% per source path. A topology memo by snapshot digest needs `GraphTables: Clone` and the reader's `checked` block set carried with it |
| O4c | Mention scan reads past the snapshot's `exclude` | By design: a mention anywhere blocks a dead-code claim; honoring `exclude` would report false dead code |
| O5a | Rest of the duplication list | `guesses.rs` vs core's concepts: `x-replacedValues` only words errors, and guesses repair, so they are not one table. `dev-unify` test hooks in dev builds: a documented build-time trade-off (without it each switch between `build:dev` and the test build recompiles tokio, hyper and every workspace crate). `ScanBudget` vs `ByteAdmission`: the same clamp, two gates; unifying them needs its tests rewritten (they read a private wait counter). `octocode-github` as the HTTP layer: a crate split, not started |
| O6 | Grammars (owner) | No grammar is unreachable, and none is redundant with oxc. Candidates: parse C++ with the CUDA grammar, a superset of its C++ base (about 3 MiB; risk: that base is 0.21 against 0.23.4, so the C++ fixtures decide); or drop CUDA (6.8 MiB measured), Scala or C#, each a contract language. Also: the engine grammar features are always on (their `cfg` arms can go), `imagesize` duplicates what zune reads, and `rmcp` alone brings 14 crates |

### Profile (mac dev build, retired instructions of the measured call)

Local tools, `before → after`:

| Flow | Before | After |
|---|---:|---:|
| astSearch match, empty (`zzqx_never_here($A)` over `crates`) | 199.9 G, 3.63 s | 12.0 G, 0.72 s (−94%) |
| astSearch match with hits | 14.2 G | 13.3 G (−6%) |
| lspSearch references, 2nd call (builds the spelling index) | 20.1 G | 16.3 G (−19%) |
| lspSearch references, 3rd+ call | 2.34 G | 2.41 G |
| localSearch, structureSearch, localFetch, astSearch symbols, graphBuild, graphQuery | | within ±3% |

GitHub and package tools, first call of a fresh server (repeat calls unchanged):

| Tool | Before | After |
|---|---:|---:|
| ghSearchRepo search / get | 1.10 / 0.72 G | 0.73 / 0.36 G |
| ghSearchCode | 3.00 G | 2.63 G |
| ghGetFileContent / with ranges | 0.90 / 0.82 G | 0.54 / 0.45 G |
| ghStructure tree / refs | 0.77 / 0.95 G | 0.40 / 0.58 G |
| ghSearchHistory commit / pullRequest | 0.80 / 0.94 G | 0.43 / 0.57 G |
| ghGetHistoryItem pullRequest | 0.76 G | 0.39 G |
| artifactSearch resolve / search | 0.67 / 0.73 G (0.20 / 0.16 s) | 0.30 / 0.36 G (0.10 / 0.09 s) |
| securityCheck package | 1.03 G | 0.66 G |

## GitHub tools (night 2)

| # | Item | State |
|---|---|---|
| G1 | First network call of every process | done. 67% of a first artifactSearch call's CPU was aws-lc seeding its random generator from CPU-jitter entropy, which aws-lc-sys always compiles at `-O0`, so release builds pay it too. `.cargo/config.toml` sets `AWS_LC_SYS_NO_JITTER_ENTROPY=1`: AWS-LC's opt-out configuration, seeded from the OS source with the CPU RNG for personalization (what ring and BoringSSL do). −0.37 G on every first GitHub, package and advisory call. Every one-shot CLI call, MCP server and warm daemon pays this once. Security-relevant build setting: review before release |
| G2 | ghSearchCode outlines every scanned file | open. 60% of a first call: the declarations extraction (tree-sitter parse 18%, `facts_from_tree` walk 49%) runs on each file with a hit, to name the declarations on hit lines. Next: a declarations walk that skips subtrees holding no hit line. Its Rust module scope, outer attributes and test-code context must stay as now |
| G3 | Repository search is never reused | owner decision. `search/repositories` answers `Cache-Control: no-cache` with no validator, and the transport honors `no-cache` (tested on purpose: `explicit_cache_directives_override_immutable_and_window_reuse`). So `SEARCH_REUSE_WINDOW` (300 s) never applies there, and a repeated identical ghSearchRepo search costs 1.6 s plus a search-quota request (30 a minute). Code search is different: `search/code` sends no cache policy, so a repeated ghSearchCode is reused within 300 s with `observedAt` disclosed (0 requests; bh-github e7, 2026-11-19). Choice, for repository search only: keep strict, or let the search window outweigh `no-cache`, disclosed as other window reuse is. The `transport/saved.rs` module doc now states both cases |
| G4 | securityCheck in profiles | not a bug: it is off unless `tools.enableSecurityCheck` (`OCTOCODE_SECURITY_CHECKS`); the profile harness drops `OCTOCODE_*`, so profiles must set it |
| G5 | ghSearchCode reads every listed file | open, design. One REST read per file of the page (core quota and latency). A GraphQL batch of blobs at the commit would be one request |


## Swarm review (night 2, 8 reviewers on the dev build)

Eight read-only reviewers ran real flows through `scratchpad/swarm/session.py` and checked answers against rg, the source, `gh api`, the registries and OSV. Their raw evidence is under `scratchpad/swarm/<area>/`. clasify used 37 billed provider requests (the owner's key).

Fixed, each with a test:

| Area | Fix |
|---|---|
| securityCheck | An unpublished or removed version that a record lists (event-stream 3.3.6, @ctrl/tinycolor 4.1.1) is reported `affected:true` with the listing ids. It used to say "advisories at other versions" without comparing anything |
| securityCheck | A version range (`^4.17.0`) warns that only its newest match was checked |
| securityCheck | A distribution check without `release` warns that every release's advisories are counted together |
| securityCheck | Repository and commit summaries keep their sentence: their `advisoryCount` is the repository's own published advisories, not the package's |
| graph | `pub(in crate::x)` no longer records a path reference, so no false imports, hubs, impact rows or cycles come from it. `GRAPH_FACTS_SCHEMA_VERSION` 33 → 34 rebuilds cached facts |
| graph | `drift` refuses a baseline built for another path, as it refuses such a head |
| graph, astSearch | Unparsed files group by type past 20 entries. They used to be one `skippedPaths` entry per file: 12,000 data files made 31 rowParts (659K chars) before any match. astSearch states them once per scan, not in every window |
| ghStructure | The folder walk has a 300-read budget per call and says so when it stops (`partialTreeFailures`, with a warning naming narrower routes). One `include` call on torvalds/linux used about 2,300 core requests and never answered |
| ghSearchCode | An empty search of an archived repository says `coverage.state:"partial"` with the reason. Other empty searches state that `complete` covers GitHub's default-branch index only |
| lspSearch | rust-analyzer `workspace.symbol.search.kind: all_symbols`: a workspaceSymbol of `flush` or `new` returned only a fuzzy-matching struct |
| lspSearch | A `let` anchored on its own name resolves to that binding (`local_binding_lines`), so its references skip the 20–70 s workspace scan and the false partial |
| localSearch | Empty-result hints name the `related[]` call they offer, not `next.asInsensitive` / `next.asRegex`, which never appear. A term whose only "regex syntax" is `-` (`fs-minipass`) is not offered a regex reading |
| clasify | Several resources each state `calls` in the summary: a two-file glob billed two requests and showed none |
| clasify | A malformed `choice` question gets the choice's own error, not the noul branch's guidance |
| selection | "classification passed 1 of 9 hits" replaces "kept 1 of 9 hits that pass the classifiers" |

Open, ranked (evidence and repro in each reviewer's folder):

| # | Area | Item |
|---|---|---|
| W1 | clasify (billing) | A `resources` glob judges every matching file, with no cap, page or estimate (`source_capture/glob.rs`: "nothing is capped"). Owner decision: cap, page under the 131K budget, or refuse above N with an estimate |
| W2 | clasify (billing) | `limit` on localSearch does not bound what classification judges: the whole walk is judged before paging (`selection/run.rs` `admit`) |
| W3 | clasify | "is this test code?" misses `#[cfg(test)]` modules; the exact `testCode` verdict is computed but not given to the judge |
| W4 | localSearch | Dense hits on one long line come back whole (a 1 MB JSON line became 57 parts): `row_window.rs` merges windows with no cap |
| W5 | localSearch | A redacted span can hide a real match, and the empty answer claims absence without a `redacted` note |
| W6 | graph | Rust calls by full path (`crate::a::B::new(`) do not link, so `dependents` misses files that lspSearch finds |
| W7 | graph | `path` routes through package nodes and reversed hops; TS overloads split into `@1/@2` and lose callers |
| W8 | artifactSearch | An old crate version gets the current repository directory (`crates/ignore` for ignore 0.4.7, which lived at `ignore/`), so the routed read 404s |
| W9 | artifactSearch | lodash 4.17.20 leads with the registry's unchecked commit, not the tag's |
| W10 | securityCheck | The summary names no advisory ids, and details cost 18–40K chars (repeated `versions` lists, three copies of one GHSA) |
| W11 | ghSearchCode | No per-file line cap (136 hit lines in one file) and `limit:3` returned 4 files |
| W12 | ghSearchHistory vs ghGetHistoryItem | `commentsCount` is 212 in one and 34 in the other (all comments vs conversation only) |
| W13 | astSearch | A content page prices every match in scope even with `limit:1` (196 G for `call_expression` over `crates`); the syntaxTree lead of an empty match often opens an import line |
| W14 | lspSearch | Trait items' target-gate warnings ignore crate reach; the Rust `diagnostic` row does not say that cargo check is off |
| W15 | Context | The tool list is 71K chars (about 18K tokens): graphQuery alone is 10.7K, and `mainGoal`/`reasoning` repeat 16 times |
| W16 | Test flake | `lsp::config::tests::catalog::a_rustup_proxy_resolves_only_when_the_toolchain_rustup_selects_ships_the_component` failed in 2 of 5 gate runs under load and passes alone. Suspected: under load the fixture settles before the first call, so its "not shipped" answer is kept, and the second check reads it. If the kept proof does not stamp the selected toolchain's component path, `rustup component add` would be missed until the answer idles out (a real bug); not yet confirmed |

The swarm session's own run of the cross-tool reviewer wrote one graph snapshot into the repo's gitignored `.octocode/graph` (a relative `--home`); it was removed and `latest` restored to the snapshot it named before (20261118T160016Z). The runner now resolves `--home` to an absolute path.

## End-to-end RFC run (2026-11-19 day, swarm of 16 agents; graph tools out of scope)

Applied in the tree (each with its checks; full gate above):
- **clasify batching (RFC TD1):** a file's search hits go in one Jev request (`tools-shared/src/selection/judge/units.rs`, generalized `requests.rs`); whole units stay one per request (GROUP_UNITS=1, fx-k2size A/B); refusal splits in halves; a missing answer is asked once more; usage sums every request. Live flowD1: 63 requests / 30,094 input tokens before, 3 / 8,171 after; neighbour-line bleed (R6) found in the first packed run and fixed. Price before billing: `Evaluator::plan_units` exists, pinned by a test, not emitted until Q2.
- **cleanup (RFC TG1):** LSP wait recorder, `module_references` chain, `skipped_cause`, `first_named` (+ `pages/remaining.rs`), `planOf`, stale doc names (about 305 lines, 3 files deleted).
- **G3 correction:** `github/src/transport/saved.rs` module doc and G3 above: code search reuses the 300 s window; only repository search is `no-cache`.

Billed and quota: clasify 70 billed requests on the owner's key (63 in one unbatched lane call that overran its 12-request cap, then 7 authorized for TD1's live check). GitHub core stayed well under the hourly limit (lanes capped at 150 each; exact counts in each lane file).

Open from this run: the RFC's tasks (workstreams A–G) and questions Q1–Q13. The RFC Markdown was also published by mistake as a private artifact (https://claude.ai/artifact/6N1DatXehcq6vd9tnKrcfU, frozen at its 13:5x version); delete or replace it on the owner's word.

## Fix wave (2026-11-19 evening, "FIX ALL TO 10"; graph tools out of scope)

| Tool | Main fixes (before → after) | Left open |
|---|---|---|
| lspSearch | TS callers find the package-name importer (was a false `complete`); first call = repeat = batch; identical repeat 20.85 G → 0.28 G | TE9 one stat per file, TE11 narrower Rust scan (owner: `outOfReachFiles`), P15 |
| localSearch | `filesScanned` exact (6,103 → 3,551 = scope); empty-search lead 760 G → 5.8 G; 3 insensitive terms 15.8 G → 2.8 G | streamed pages re-search past each cut (~40% est.); matchOnly value omission |
| localFetch | minified line 9 calls → 1; batch water-fill 3 files 4 → 3 calls | — |
| structureSearch | details −19% per entry; sort-error example fixed | — |
| astSearch | `limit:1` 195 G → 3.1 G; repeat 12.4 G → 3.0 G; 5-rule batch 24 G → 2.8 G; Python outline 7.8K → 2.3K chars | 5 identical cold rows 8.1 G (target 6); shared walk across rules (P14) |
| ghSearchCode | page 14 core → 2 GraphQL; follow-up read 0 requests; repeat 282 M → 88 M | TE5 outline near hits only; repeat ≤ 30 M (rest is response serialization) |
| ghGetFileContent / ghStructure | 3-file batch 6 → 1 core + 1 GraphQL; refs repeat 0.86 s → 0 requests; 404 names the ref | — |
| ghSearchHistory / ghGetHistoryItem | counts agree (222 + 120 review); bots filter, conversation first; 3 PRs 3 → 1 GraphQL | 3-PR wall 1.46 s (target 1.3) |
| artifactSearch | monorepo tags, crate directories, PyPI repo fallback | — |
| securityCheck | call-scoped OSV share (Debian 42 → 27); advisory ids in summary; repo repeat 51 → 0 OSV reads | details markdown/twin deltas (owner, Q8) |
| clasify | per-file packing (63 → 3 requests, −73% tokens); price before billing + `maxRequests`; `perHit` opt-in (owner); read call per file; details −53% | sections rows unpriced; question-text hoist needs an A/B |
| all | core text package (AGENTS.md/README orientation, check-before-fetch when securityCheck is on); one hoist stage (shared/omitWhen/groups/outline strings after redaction); 0 broken rustdoc links | Rust splits TF1–TF9 (later wave) |

Security incident during the wave: outline strings built before redaction leaked a context-detected secret in the 15:53 dev build; fixed (strings only in the hoist stage, a `Judged` proof token, guard tests).
Owner decisions: securityCheck stays off by default; clasify keeps packing with a `perHit` opt-in; CI workflows, `.octocode/`, AGENTIC.md, CLAUDE.md, LSP.md and the release guide deleted.
Billed clasify this day: 75 requests (63 overrun + 7 + 5 live checks).
