# TODOs (2026-11-18)

This lists every open item from round 3 (tool-ratings fixes) and the velocity work. Each row gives the evidence and the next step. The owner asked for this file; delete it when the owner says so.

**State of the tree:**
- Nothing is committed.
- The tree passes `yarn workspace @octocodeai/octocode-native verify:own` (317 s) and `dev.mjs lint:rust`.
- The live pin bin-r4o passes all six scorecard P0/P1 items: `packages/octocode-benchmark/campaigns/20.2.0-2026-11-16-round3/final-validate/rows.jsonl`.

**Plans:**
- Velocity: `.octocode/octocode-rfc-generator/2026-11-18-velocity-first/RFC.md`.
- Round 3: `packages/octocode-benchmark/campaigns/20.2.0-2026-11-16-round3/ROUND.md`.

## Owner decisions

| # | Decision | Evidence | If yes |
|---|---|---|---|
| D1 | Review and commit the tree. It holds the crate split, round 3, PEM-PREFIX, APPEND-PREFIX and the velocity work | `Cargo.lock` changes only the 7 removed dependency edges (no version or package change); `yarn.lock` changes only the 2-line core content hash per contract regen | owner commits |
| D2 | Enable Developer Tools for the terminal (Ghostty) and VS Code: `sudo spctl developer-mode enable-terminal`, then System Settings → Privacy & Security → Developer Tools | A focused run right after a rebuild takes 64.2 s against 12 s for a rerun; macOS assesses each newly linked test binary | rerun the focused-loop check (RFC V2) |
| D3 | A/B a releasing allocator (mimalloc) for the warm daemon | One cold `lspSearch references` in this repo leaves the daemon at 680–707 MB (23 MB with `workspaceRoot` at the fixture). `vmmap`: 161 MB live, 138 MB fragmentation, about 445 MB freed but held. `malloc_zone_pressure_relief` releases 0 bytes on Darwin 25, so `engine::memory::release_freed_memory` is a no-op here. The old pin behaves the same | RFC T8: build with mimalloc in a scratch copy; compare footprint, peak and the tool profile |
| D4 | Keep `lsp.prewarm` default `targeted`? | GRAPH-AST-Y-2 kept the documented default; the question was left open | none if kept |

## P1: velocity (RFC 2026-11-18-velocity-first)

| # | Item | Evidence | Next step |
|---|---|---|---|
| V1 (T9) | Heaviest test tails | Side by side: `tool-local` 121–214 s (`local_search::tests::streamed` ×2 and `pages::large_scan` each over 60 s), `tool-graph` 73–116 s, facade integration 33–163 s | Per-test nextest ranking, then injectable thresholds or smaller fixtures |
| V2 (T10) | Audit the libtest binaries for tests that start child processes | The side-by-side load exposed three that hung or failed (`stderr_ring`, `gh_git::process::tests`, fake rustup); all moved to `PROCESS_TESTS` | `rg 'Command::new\|spawn'` in test modules not matched by `PROCESS_TESTS`; move each |
| V3 (T11) | `test:dev --crate X --lib` still rebuilds the facade and CLI | 30.7 s from edit to result; 6.2 s warm | Workspace feature unification (cargo-hakari, or the nightly `-Zfeature-unification`) |
| V4 (T7) | `#[path]` mounts compile shared fixtures in three crates | runtime-base `test_support` is mounted by runtime tests; runtime `tests/support` by cli tests and benches. `cargo fix` narrowed the shared files into errors (restored) | Move the fixtures into the `octocode-test-support` crate |
| V5 (T12) | An engine edit rebuilds 14 crates | 115–129 s, 317–334 CPU s | Measure, then split the engine by domain |

## P2: round-3 leftovers (the owner limited round 3 to P0/P1)

| # | Item | Evidence | Next step |
|---|---|---|---|
| R1 | SMALL-4: the hover redacts a value the file read serves (HOVER-JUDGE); ghSearchCode count-change wording, option (a) | Unverified partial patch: `…/round3/small-4/SMALL-4-PARTIAL.patch`; brief `…/roast-validate/lanes/SMALL-4.md` | Finish on a fresh copy, with tests |
| R2 | LOCAL-Z: per-term counts of overlapping terms; one walk on the 214 MB line; the empty-search hint counts `.bin` twice; structureSearch entries without a `reason` | Brief `…/roast-validate/lanes/LOCAL-Z.md` (needs APPEND-PREFIX, now landed) | Run the lane |
| R3 | CLEAN-1 (5 small items), ENGINE P2, HEAVY-2, CLEAN-2 remainder | `.octocode/build-audit/2026-11-17/fix-all/{CLEAN-1,HEAVY-2}/`; the copies sat in another session's scratchpad (likely wiped) | Redo from the briefs |
| R4 | Stale-socket startup flake | `cli::serve::unix::tests::startup::a_stale_socket_is_replaced_and_a_live_one_is_existing`; likely the macOS socket+FIOCLEX race (a child inherits the listener). A daemon's child holding the socket is also a product edge | A handshake liveness probe; then a deterministic test |
| R5 | drift on an unrebuilt graph says `status: empty` | Only `freshness.fresh: false` tells; no hint to rebuild first (`graphQuery drift`, live on bin-r4o) | A rebuild hint on the stale answer |
| R6 | PPK MAC false positive | ``key_markers.rs:12 //! `Private-MAC:` closes it.`` hides 12 doc characters (the only added FP in 17,531 files) | Require a hex MAC value after `Private-MAC:` |
| R7 | A load-sensitive engine test | `search::text_search::tests::streaming::page_one_lists_no_directory_past_its_cut` failed at opt-level 1 (8 directories against the bound) | Make the bound independent of worker speed |
| R8 | The core `importerScan` enum still lists `complete`; error Examples echo every valid field (a huge field would be echoed); the rowPart join rule is missing from agent text; lodash GitHub records listed twice | `HANDOFF-2026-11-18.md` "Held / open" | A core edit and regen for the enum and the text; the duplicate at its source |
| R9 | Small instruction increases | ghGetFileContent +21%, ghStructure +18%, ghSearchHistory +14% (single samples, `final-validate/rows.jsonl`); lspSearch `implementation` 75 → 122 M warm (intel-4 filter, 0.08 s) | Re-sample; accept or trim |
| R10 | Re-rate the scorecard | `packages/octocode-benchmark/results/rounds/20.2.0-2026-11-18-tool-ratings/SCORECARD.md` predates the fixes | Re-rate the six P0/P1 rows on bin-r4o |

## Housekeeping

| # | Item | Note |
|---|---|---|
| H1 | `.octocode/` is 10 GB of reports and data (20 GB before today's cleanup: 75 workspace copies, temp folders and 6,041 copied code and script files removed) | Optional: archive rounds older than a month |
| H2 | Two 5-day-old `node …/octocodecode/…/scratchpad/L16b/tools/prov.mjs` processes | Another project's session; check and stop them by PID if unneeded |
| H3 | Pins kept: `…/round3/bin-r4l` (scorecard baseline) and `bin-r4o` (final), both git-ignored; r4m and r4n binaries deleted (sha and logs kept) | Delete after re-rating (R10) |
