# Rate round 4: GitHub history lane (ghSearchHistory, ghGetHistoryItem, ghCloneRepo)

Build: `octocode 20.2.0+56f67471`, `packages/octocode-native/npm/darwin-arm64/octocode` SHA-256 `e246ff42592b8192da65589ef39a7f573f04f4643b15765e9d98fb5cff6a405f`, HEAD `e9309aa35`, dirty tree (97 porcelain lines). CLI `--json` only. Ground truth: `gh api` (GET), `git` in the clones.

## Table

| tool | Desc | Schema | Workflow | Clarity | Time | Tokens | Paging | Smart | Short | Long | Stress | Hints | Overall | Correct |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| ghSearchHistory | 9 | 9 | 9 | 9 | 9 | 9 | 10 | 9 | 9 | 9 | 9 | 9 | **9** | 10 |
| ghGetHistoryItem | 8 | 9 | 9 | 8 | 8 | 8 | 7 | 9 | 9 | 8 | 8 | 9 | **8.5** | 9.5 |
| ghCloneRepo | 9 | 9 | 9 | 9 | 9 | 10 | – | 9 | 9 | 9 | 9 | 9 | **9** | 10 |

Spent: ~300 CLI calls (22 logged single calls ≈93K chars; walks: tokio 5 pages, ripgrep#1017 3×11–14 pages, TypeScript#51387 2×85 pages, kubernetes#141224 27 pages, ripgrep compare 9 pages, ~45 read continuations), ≈17M chars, almost all from walks at `responseChars:100000`. GitHub search quota: 4 of 30/min used at most.

- ghSearchHistory: in: (question) / ghGetHistoryItem commit related[] → ghSearchHistory → out: ghGetHistoryItem
- ghGetHistoryItem: in: ghSearchHistory, ghStructure refs → ghGetHistoryItem → out: ghGetFileContent (readAtCommit/readParent), ghSearchHistory (PR-by-SHA related)
- ghCloneRepo: in: ghSearchRepo, ghStructure refs → ghCloneRepo → out: localSearch/localFetch/structureSearch at location.path

## Checked answers (all matched ground truth)

| Call | Octocode | Ground truth |
|---|---|---|
| PR search fd `hyperlink` isMerged (cold 0.67s, warm 0.54s) | 5 rows, same order | `search/issues` 5, same numbers |
| Batch 5 (commit kw+committedAt, commit path list, issue labels, PR author+mergedAt, PR kw), 8.1s | rg multiline 2018: 3; flask path: 5 shas/authors/dates; tokio JoinSet A-tokio open: 5; williammartin June 2024: #9256; io_uring merged total 46 | identical (`search/commits`, `commits?path=`, `search/issues`) |
| Walk io_uring merged, limit 10 | 5 pages, 46 unique, total 46 | same 46 numbers |
| Stress: rust-lang/rust PR `fix` limit 100 | countScope capped, total 1000, warning "GitHub matched 46989" | total_count 46989 |
| commit ref v10.2.0 + path src/hyperlink.rs; bat createdAt asc; cli headRef trunk (183); commit author email | all match | `commits?sha=&path=`, `search/issues head:trunk` 183, `commits?author=` |
| PR read fd#1571 (cold 0.43s, warm 0.03s cache with observedAt) | author, mergedBy tmccombs, counts 6/7/5/12/+215/-34 | `pulls/1571` identical |
| Batch 5 PR/issue sections | comments 1 of 4 (paged), reviews 9, reviewComments 7, commits 5; regex matchString empty ("0 of 6 files") | 4, 9, 7, 5; rg on raw src patch: 0 hits |
| matchString `PathUrl` | hyperlink.rs 5, output.rs 2 | raw patch 7 (5+2) |
| Commit 9e79f8c2 summary/patches | committer + committedAt in both views; related PR-by-SHA → #1017 | `commits/…/pulls` → 1017 |
| Compare fd v10.1.0..v10.2.0 | aheadBy 52, 17 files, 52 commits in default view | ahead_by 52, 17 files |
| Compare ripgrep 0.10.0..14.0.0 commits walk | 9 pages, 847 commits | paged compare: 847, same set and order (`cmp`) |
| Walk ripgrep#1017 patches | 14 pages, 109 files (next.files on last patches page) | 109 files |
| Clone fd@v10.2.0 cold 3.0s / warm 1.6s reused | commitSha b1913687, 54 files, 543764 bytes, clean | tag → b1913687; tree 54 blobs; sum of sizes 543764 |
| Clone batch (sparse rg, depth 5, bad ref, bad repo, sparse flask) 6.4s | sparse sets exact; errors notFound with default-branch + ghStructure refs lead | disk == GitHub tree (rg 16, flask 35) |
| Stress: kubernetes sparse pkg/kubelet, TS short-SHA file scope, `owner:"github.com/sharkdp"` 10.3s | 824+20 root files minus 4 disclosed `skipped` keys; 2 symlinks disclosed; short SHA resolved; URL owner repaired with warning | `git ls-tree` diff = exactly the 4 skipped files |

## Defects (most severe first)

1. **med, native — ghGetHistoryItem: withheld-patch files after the first response page get no read continuation.**
   Input: `{"responseChars":100000,"queries":[{"operation":"pullRequest","owner":"kubernetes","repo":"kubernetes","number":141224,"sections":["patches"]}]}` and the same for `microsoft/TypeScript#51387` with `include:["src/**"]`.
   Observed: kubernetes: 22 files `patchUnavailable` + `terminalLimit:true` on pages 6, 12, 20 of 27; zero `readAtCommit`/`readParent` anywhere. TypeScript: 238 withheld; reads (9+9 batches) only on page 1 and cover 42 files per side; the 196 withheld files on pages 18, 29, 53, 61, 82 get none. `terminalLimit` says "unreachable by any call" though ghGetFileContent reads each at head/base.
   Expected: every withheld file gets a head and/or parent read on the page that lists it.
   Ground truth: `gh api repos/kubernetes/kubernetes/pulls/141224/files --paginate` → the same 22 files with no `patch`.
   On ripgrep#1017 (all 12 withheld files on page 1) coverage is now complete: head reads 7/7 (added+modified), parent reads 7/7 (removed+modified).
2. **low-med, core+native — ghSearchHistory rows cannot carry mergedBy.** `fields:["mergedBy"]` on tokio `io_uring` → warning "No row has mergedBy". ghGetHistoryItem now has `mergedBy` (fd#1571 = tmccombs = `pulls/1571 .merged_by`).
3. **low, core — `historyDepth` "Commits of history" undercounts merges.** fd@v10.2.0 `historyDepth:5` → `git rev-list --count HEAD` 13, first-parent 5 (git depth counts generations).
4. **low, native — the commit patches view drops the author's name/email** that the summary view shows (9e79f8c2: patches `author:{login,authoredAt}`; summary has name+email). Committer is complete in both.
5. **low, native — warning grammar:** "only src/compiler/checker.ts are on disk" for a one-path scope.

Not a defect (checked): a label search leaves out the filtered label from row `labels` — documented in `--view output` ("a search leaves out labels its filters require").

## Regression check (round 3)

| Id | Status | Evidence |
|---|---|---|
| R3-H1 | partly fixed | ripgrep#1017: 4 read cursors batch all 12 withheld files (7 head, 7 parent). Later pages still get none: defect 1 |
| R3-H2 | partly fixed | ghGetHistoryItem `mergedBy` present; search rows still lack it |
| R3-H3 | fixed | 9e79f8c2 `sections:["patches"]` has committer with committedAt 2018-08-19T14:55:43Z (= `commit.committer.date`) |
| R3-H4 | fixed | `"(unclosed"` rust → error "Invalid `matchString`: … unclosed group" |
| R3-H5 | fixed | `keywords:["author:sharkdp"]` → hint "set `filters.author` to `sharkdp`" |
| R3-H6 | fixed | fd compare default view lists all 52 commits |
| R3-H7 | fixed | ghCloneRepo description: "A few paths: ghStructure with materialize:true." |

## Good

Paging is exact: 847 compare commits, 46 search rows and 109 PR files rebuilt once, in GitHub's order. Caps are disclosed with GitHub's true count. Clone checkouts match the tree byte-for-byte, and their exclusions (credential files, symlinks) are disclosed.

## Clones made (under ~/.octocode/clones, outside the repo)

- sharkdp/fd b1913687 (v10.2.0), sharkdp/fd b1913687 depth 5, sharkdp/fd b80cd53e (master)
- burntsushi/ripgrep e082e98b sparse crates/searcher + README.md
- pallets/flask 49943b5a sparse src/flask
- kubernetes/kubernetes 539fda3e sparse pkg/kubelet
- microsoft/typescript da6f0671 sparse src/compiler/checker.ts
