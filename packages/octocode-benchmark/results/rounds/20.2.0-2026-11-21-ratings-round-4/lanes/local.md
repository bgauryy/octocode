# Rate round 4: Local lane (structureSearch, localSearch, localFetch)

Build: `packages/octocode-native/npm/darwin-arm64/octocode`, `octocode 20.2.0+56f67471`, sha256 `e246ff42592b8192da65589ef39a7f573f04f4643b15765e9d98fb5cff6a405f` (unchanged at the start and end of the lane), HEAD `e9309aa3`, dirty tree (97 entries in `git status`). CLI `--json` only.
Spend: about 143 CLI calls (21 single calls plus 122 page-walk calls), about 1.9 M chars. 1.15 M of that is one deliberate stress walk of a 1 MB file.

## Table

| tool | Desc | Schema | Workflow | Clarity | Time | Tokens | Paging | Smart | Short | Long | Stress | Hints | Overall | Correct |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| structureSearch | 9 | 9 | 9 | 8 | 9 | 9 | 10 | 9 | 9 | 9 | 9 | 8 | **9** | 10 |
| localSearch | 9 | 9 | 9 | 8 | 9 | 8 | 10 | 8 | 9 | 9 | 9 | 9 | **9** | 10 |
| localFetch | 9 | 9 | 9 | 8 | 10 | 8 | 10 | 9 | 9 | 10 | 10 | 9 | **9** | 10 |

- `in: (start), localSearch → structureSearch → out: localFetch, localSearch`
- `in: structureSearch → localSearch → out: localFetch, lspSearch`
- `in: localSearch, structureSearch → localFetch → out: clasify (images), structureSearch (dirs)`

## Checks against ground truth (all matched)

### structureSearch

- Top 5 `.rs` files by size under `crates`: the paths and byte counts match `find | stat`. The total, 3510, matches `git ls-files`.
- A 7-page walk of `tool-local` returned 243 entries. The set equals `git ls-files -co --exclude-standard`, with no duplicates.
- `nameRegex` ARCHITECTURE.md: 10 files. The other 12 that `find` sees are all gitignored (`git check-ignore`) and appear in `skippedPaths`. With `includeIgnored` it returns 22, which equals `find`.
- `modifiedAt {from:-PT6H}`: 153 files, which equals `find -mmin -360`. The order matches `stat`.
- `totalLines` sort matches `wc -l`. `include **/Cargo.toml` returns 17, which equals `find`.
- Hidden entries: 11. The 4 missing entries are gitignored and counted in `skippedPaths`.
- Errors (bad path, bad regex, notAFile) give correct codes and useful `related[]`.

### localSearch

- `countMatches`: `errorCode` 797 / 301 files, equal to `rg`. Two terms: 96, equal to 8 + 88.
- `files`, `filesWithout` (178, equal to `rg --files-without-match`), `countLines`, multiline (4), `keyPath` and `unique` all match.
- A paged content walk (limit 10) returned 14 hits. They equal `rg -n` exactly, each once.
- Multi-row batch: response-level and row-level continuations both rebuilt every row exactly once.
- Repo root `fn ` across 5515 files: 6 parallel calls gave identical results in 2 s wall. Every per-file difference from `rg` is explained:
  - live `chrome-devtools` churn;
  - smart case (`Fn `, documented);
  - 1 masked span (documented `maskedSpans`).
- `huge-go` with `includeIgnored` (31k files): 39, equal to `rg --no-ignore`, in 2.2 s. The `binaryQuit` partial is disclosed.

### localFetch

- A 130 KB file walked in 9 pages: the rebuilt file is identical by `cmp`.
- A 1 MB single-line JSON walked in 63 byte pages through the `related` range read: identical by `cmp`.
- `matchString` with context and `limit: 40`: 15 pages, 571 lines each shown once, every line text equals the source. 155 matchedLines, equal to `rg`.
- `block` returns 267–287 (the doc comment plus the fn), which is correct.
- Ranges merge overlaps. A range past the file end is warned about.
- Directory, binary PNG (routes to clasify) and missing file all get correct errors.

## Defects (all low)

| # | Tool | Input | Observed vs expected | Evidence | Owner |
|---|---|---|---|---|---|
| L4-1 | localFetch | `{"path":"skills/octocode-scraping/scripts/tests/har-ingest.test.mjs","matchString":["REDACTED"]}` | Warning says "3 lines match ignoring case." The case-insensitive rerun shows 2 lines (111, 168) with 3 matches (`selectedMatchCount: 3`). The warning counts matches but calls them lines. | `rg -i -c -F redacted` gives 2 | native |
| L4-2 | localFetch | `view:"symbols"` on `tool-local/src/structure_search/tree.rs` | The macro item shows as `20\tsuper::listing::listing_fields!(… { …` with no `20-73` extent. Other items show `13-46`. Its 6 inner fns are not listed. | `sed -n 20,73p`; 7 `fn ` in file vs 1 listed | native |
| L4-3 | structureSearch | `modifiedAt:"P1D"` | The refusal "Expected object {from, to}" comes with an example that has no `modifiedAt`. Expected: an example like `{"from":"-P1D"}`. | `ss5` row 2 | native (example builder) |
| L4-4 | localSearch | `countMatches`, `matchString:["scopeTooBroad","staleSnapshot"]` | `termCounts` appears only per file, so per-term totals need every page walked and summed. The row has `matchCount` but no row-level `termCounts`. | row keys `matchCount, files, maskedSpans, matchedFiles, filesScanned` | core + native |

## Regression check (round 3)

| id | Status | Evidence |
|---|---|---|
| R3-L1 | fixed | localFetch gives `AWSACCESSKEYID` 0, `REDACTED` 0 (case-sensitive) and `TOKEN` 19. `rg` gives 0, 0 and 19. |
| R3-L2 | fixed by disclosure | `ghp_` still gives 1 vs `rg` 2, and 15 vs 18 over `engine`. Each row and file now carries `maskedSpans`, and the output schema says "hits inside them are not matched or counted". An empty row with no masks no longer says "masked secrets are not searched" (tree.rs miss). |
| R3-L3 | partly fixed | The macro item now shows with `…` but has no extent and no inner methods (L4-2). |
| R3-L4 | fixed | The empty row under `testCode:"exclude"` says "left out 90 file(s)" and gives a `related` call with `testCode:"include"`. `only` gives 14, equal to `rg`. |
| R3-L5 | mostly fixed | The output schema now documents `matchedLineCount` omission, `value`, and `testCodeExcluded` under `only`. The P1D example is not fixed (L4-3). The sparse-clone hint was not re-tested (it needs ghCloneRepo, another lane). |
| R3-F3 | fixed | `\[[A-Z-]+\]` over the placeholder-only file is empty with no case-insensitive hint. |

## Good

- Every count, set and byte rebuild matched ground truth exactly, including the 1 MB single line and parallel calls.
- Coverage (`skippedPaths`, `maskedSpans`, `binaryQuit`, `testCodeExcluded`) is always stated with counts.
- Empty and error rows carry an executable next step.
