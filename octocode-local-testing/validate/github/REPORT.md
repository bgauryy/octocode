# Octocode GitHub tools vs expert `gh`: head-to-head

Date: 2026-09-30. Versions: `octocode 19.2.0 (native 20.0.0)` and `gh 2.96.0`, both using the same account and token.

The raw data is in this folder:
- `runs.jsonl` and `results.json` hold every call and the totals per task.
- `tasks.json` holds the tasks, their ground truth and the grading.
- `out/` holds the raw stdout of every call and every replayed continuation.
- The scripts are `run.mjs`, `follow*.sh`, `crawl.mjs` and `aggregate.mjs`.

## Method
- **Chars read:** stdout + stderr code points, meaning what a shell tool hands back to an agent. Wall time includes process start: the floor is about 155 ms per octocode call and about 28 ms per gh call.
- **The gh side** used expert commands: `gh search … --json --jq`, `gh api` with `--jq`, `--paginate`, `-H 'Accept: application/vnd.github.raw'` with `awk`/`sed -n`, recursive trees, `gh pr view --json`, and `compare --jq`. For package lookups the baseline was `npm view` or `curl | jq`. When an expert would retry after a failed first attempt, the retry counts as an extra call.
- **The octocode side** used the native CLI with expert queries (`concise`, `fileFilter`, `matchString`, `pageSize`). Continuations were replayed verbatim.
- **Ground truth** was checked independently of both sides, using raw fetches with `grep -n`, search `total_count`, a full diff of the recursive trees (786/786), and a blobless clone for the true compare file count (339).

## Headline (31 tasks)
| | chars read | calls | wall ms | fully correct |
|---|--:|--:|--:|---|
| gh expert | 105,823 | 34 | 34,122 | 28/31 |
| octocode | 270,272 | 52 | 71,206 | 30/31 |

- gh's 3 misses: g3 found 2 of 4 files without saying so, g5s reported 300 files as complete, and fc3 gave no commit SHA.
- octocode's miss: g3, the same 2 of 4, also without saying so.
- Most of octocode's extra chars come from 2 tasks. g5full (full compare) took 8 calls and 145,743 chars, and g6 (full issue thread) took 3 calls and 45,004 chars.

| Tool | gh chars/calls/ms | octocode chars/calls/ms |
|---|---|---|
| ghSearchCode | 1,382/8/6,797 | 3,172/6/8,058 |
| ghSearchRepo | 423/2/2,435 | 2,375/2/2,120 |
| ghStructure | 27,460/4/3,872 | 34,269/13/19,577 |
| ghGetFileContent | 4,523/3/2,010 | 8,137/3/4,987 |
| ghSearchHistory | 4,568/4/3,004 | 7,158/4/7,500 |
| ghGetHistoryItem | 66,593/8/12,008 | 211,343/19/25,168 |
| artifactSearch | 874/5/3,996 | 3,818/5/3,796 |

## Per-tool results
Numbers are chars/calls/ms.

### ghSearchCode
| Task | gh | octocode | Verdict |
|---|---|---|---|
| cs1: where the tokio bounded mpsc acquires its semaphore (bounded.rs:1302) | 222/1/1602 ✔ | 623/1/1662 ✔ | Tie. gh uses 2.8x fewer chars; octocode adds an exact `next.readTopMatch`. |
| cs2: `impl Future for Acquire` | the quoted query returned 0, the retry worked: 198/2/1394 ✔ | 605/1/1538 ✔ | Octocode handles the phrase on the first try. |
| cs3: repo renamed (facebook/react → react/react) | returned `total 0, incomplete` silently; found only after manual discovery: 315/3/2271 | `next.retryRenamed`: 742/2/2942 ✔ | **Octocode wins.** |
| cs4: path search paging, total 10 | 448/1/826 ✔ | 950/1/1000 ✔ | Tie; both report honest totals. |
| cs5: which tokio-rs repo defines `pub struct Router` | 199/1/704 ✔ | 252/1/916 ✔ | Tie. |

### ghSearchRepo
| Task | gh | octocode | Verdict |
|---|---|---|---|
| rs1: most-starred Rust HTTP client | 320/1/1770 ✔ | 1944/1/982 ✔ | Same answer. Octocode's full rows cost 6x the chars. `concise` costs 466 chars but drops the star counts. |
| rs2: top 5 tokio-rs repos | 103/1/665 ✔ | 431/1/1138 ✔ | Tie. Octocode excludes archived repos by default. |

### ghStructure
| Task | gh | octocode | Verdict |
|---|---|---|---|
| st1: the 7 files in mpsc | 140/1/1714 ✔ | 220/1/1484 ✔ | Tie. |
| st2: the 41 files under sync | 740/1/380 ✔ | 933/1/1289 ✔ | Tie. |
| st3: all 786 files in kubernetes pkg/kubelet | 26,524/1/1,409 ✔ | 31,131/10/15,716 ✔ | **gh wins.** Octocode's page 1 is 100 folders and 0 files. |
| st4: 3 largest files | 56/1/369 ✔ | 1985/1/1088 ✔ | **gh wins.** Octocode has no sort or top-N. |

### ghGetFileContent
| Task | gh | octocode | Verdict |
|---|---|---|---|
| fc1: `reserve_inner` at a pinned SHA | 1731/1/608 ✔ | 3183/1/878 ✔ | gh's awk stops at the closing brace; octocode's symmetric window adds doc lines. |
| fc2: a function in the 1.45 MB checker.go | 1301/1/1055 ✔ | 3049/1/2922 ✔ | Tie on correctness. |
| fc3: a known range plus the commit it came from | 1491/1/347, partial (no SHA) | 1905/1/1187 ✔ with `commitSha` | **Octocode wins** on reproducibility. |

### ghSearchHistory
| Task | gh | octocode | Verdict |
|---|---|---|---|
| h1: 38 merged react PRs mentioning useEffect in 2025 | 2942/1/1253 ✔ | 2674/1/4247 ✔ identical set | Tie. Octocode's concise rows carry no dates. |
| h2: open tokio issues mentioning JoinSet | 422/1/869 ✔ | 525/1/1249 ✔ | Tie. |
| h3: last 10 commits touching bounded.rs | 1097/1/453 ✔ | 3418/1/837 ✔ | Same answer at 3.1x the chars. There is no concise mode for commits. |
| h4: the commit that added `blocking_recv_many` | 107/1/429 ✔ | 541/1/1167 ✔ | Tie. |

### ghGetHistoryItem
| Task | gh | octocode | Verdict |
|---|---|---|---|
| g1: PR state, SHAs and files | 349/1/492 ✔ | 969/1/1147 ✔ | Tie. Octocode adds exact next steps. |
| g2: the 24 files under transformers in a 656-file PR | 1364/1/3348 ✔ (only via `--paginate`) | 1315/1/2195 ✔ with `!omitted`/`!tooLarge` flags | **Octocode wins.** `gh pr view --json files` silently caps at 100, and `gh pr diff --name-only` fails with HTTP 406. |
| g3: patches that add an import (4 files) | 159/1/3804, 2 of 4 without saying so | 3366/2/3373, 2 of 4 without saying so | Both miss the files that have no patch (D1, D2). |
| g4: a commit's file list plus one diff | 1772/1/421 ✔ | 3644/2/1647 ✔ | **gh wins** (D6). |
| g5s: compare counts and completeness (339 files) | 153/1/1234, says 300 with no cap signal | 9476/1/1227 ✔, `isPartial` plus `providerFileLimit` | **Octocode wins on honesty.** |
| g5full: all 240 commits and 300 files | 30,189/1/1,492 | 145,743/8/11,449 (following every continuation: 12 calls, 197,947 chars) | **gh wins by far** (D4). |
| g6: issue body plus 10 comments | 31,063/1/741 | 45,004/3/2,854 | **gh wins.** Octocode splits the body and the rows. |
| g7: one PR patch | 1544/1/476 ✔ | 1826/1/1276 ✔ | Tie. |

### artifactSearch (baseline: npm view or curl | jq)
| Task | baseline | octocode | Verdict |
|---|---|---|---|
| a1: npm zod | 90/1/361 ✔ | 484/1/689 ✔ | Tie. Octocode normalizes the URL and adds `next.viewRepo`. |
| a2: pypi requests | 129/1/364 ✔ | 407/1/592 ✔ | Tie. |
| a3: crate serde | 59/1/1164 ✔ | 441/1/1268 ✔ | Tie. |
| a4: npm react (a monorepo) | 104/1/822 ✔ | 513/1/642 ✔ with `repositoryDirectory` | Slight octocode edge. |
| a5: discover YAML parsers | 492/1/1285 | 1973/1/605 | Tie: both return the same 5. |

## Edge cases
| Case | gh | octocode |
|---|---|---|
| Nonexistent repo | 404, exit 1 | `notFound`, exit 3; the hint talks about path case (D8) |
| Nonexistent path | 404 | `notFound` plus `next.viewTree` of the parent folder; useful |
| Binary PNG | dumps raw bytes into context | `decode` "Binary file detected"; the hint is cut off (D7) |
| Huge 2.35 MB file | an expert must know not to cat it | `isPartial`, a size-limit flag, `commitSha`, and a continuation pinned to that SHA. Best in class. |
| Private repo, file read | 404 | 404 with no private-or-access hint (D8) |
| Private repo, code search | empty stdout, exit 0 | `providerIncompleteResults` plus a pointless `next.retry` (D9) |
| Invalid token | 401 | `authentication`, exit 4, names the env vars; the hint is cut off (D7) |
| PR number that is really an issue | GraphQL error | generic `notFound` with no `next.readIssue` (D11) |
| Nonexistent branch, tree | 404 | **silently serves the default branch, exit 0** (D3) |
| Nonexistent branch, file | 404 | clear ref-not-found error, exit 3 |
| Zero results (code, PR or repo search) | empty, exit 0 | `status:empty`, exit 1, with a hint |
| Nonexistent npm package | E404 | `empty` instead of `notFound` (D12) |
| Text only on a non-default branch | empty | empty, with no default-branch-only note (D13) |
| Rate-limit burst of 12 calls | 403 with no reset time | a local circuit breaker ("request not sent", about 230 ms) with `retryAfterSeconds` and `resetEpochSeconds`. Best in class. |

## Ratings
| Tool | Rating | Why |
|---|--:|---|
| ghSearchCode | 8 | Matched gh on 5/5, and does better on renamed repos and phrases; 2.3x the chars |
| ghSearchRepo | 7 | Honest totals and archived filtering; full rows cost 6x, and `concise` drops stars |
| ghStructure | 6 | Folders-first paging takes 10 calls where gh takes 1; no sort or top-N; silently falls back to the default branch |
| ghGetFileContent | 8 | Returns `commitSha`, SHA-pinned continuations, and handles binary and huge files; about 2x the chars |
| ghSearchHistory | 8 | Identical result sets; commit rows cost 3.1x, with no concise mode for commits |
| ghGetHistoryItem | 6 | Wins on large-PR file lists and compare honesty; loses on full compare and full issue reads; D1 and D2 |
| artifactSearch | 8 | Correct on npm, PyPI and crates; 4–7x the chars; a missing package reports `empty` |

## Wins and losses
**Wins:**
- Safety signals gh doesn't give:
  - renamed-repo recovery;
  - compare and file caps;
  - flags for files with no patch;
  - `commitSha` on reads that aren't pinned;
  - SHA-pinned continuations;
  - rate-limit data;
  - binary detection;
  - archived repos filtered out.
- Large PRs, where gh's obvious commands cap at 100 files or fail with 406.
- Distinct exit codes and continuations.
- Phrase search that works on the first try.

**Losses:**
- 2.6x the chars overall, from JSON envelopes, verbose rows, symmetric windows, and having no projection, sort or top-N.
- Multi-page reads: 10, 8 and 3 calls where gh needs 1.
- Latency: a median of 1062 ms against 645 ms.
- Some silent failures remain (D1–D3).

## Defects
- **D1 (medium):** PR `matchString` is ignored unless `content.patches` is set. `ghGetHistoryItem {"operation":"pullRequest","owner":"microsoft","repo":"TypeScript","number":51387,"matchString":"_namespaces/ts","matchContext":0,"fileFilter":{"paths":["src/compiler/transformers/module/**"]}}` returns only the summary.
- **D2 (medium):** a patch search silently skips `!tooLarge` files that have no patch. `module.ts` and `system.ts` were missed, with no `isPartial` and no list of files that weren't searched.
- **D3 (medium):** ghStructure silently falls back to the default branch on a ref that doesn't exist, and still exits 0. Repro: `ghStructure {"owner":"tokio-rs","repo":"tokio","branch":"no-such-branch-zz","path":"tokio/src/sync/mpsc","maxDepth":1}`.
- **D4 (medium, efficiency):** compare pagination sends data again. Every file page repeats commit page 1, the commit continuations carry `filePage`, and a row split repeats the whole file array. Repro: `ghGetHistoryItem {"operation":"compare","owner":"tokio-rs","repo":"tokio","base":"tokio-util-0.7.18","head":"tokio-util-0.7.19","pageSize":100}`.
- **D5 (low):** compare reports `countScope:"complete"` with `totalFiles:300` next to `providerFileLimit` (the true count is 339). `base` is resolved to a SHA, but `head` stays a tag.
- **D6 (low):** a commit read scoped to one path returns `changedFiles:1` with a scope of "complete", but the additions count covers the whole commit, which changed 2 files.
- **D7 (low):** the hints for authentication failures and binary files are cut off mid-sentence with "…".
- **D8 (low):** a repo-level 404 gives only path-case guidance, suggests `viewTree` on the same missing repo, and links the wrong `documentationUrl`. There is no "the repo may be private, or the token lacks access" branch.
- **D9 (low):** code search on a repo you can't access suggests `next.retry`, which can't help.
- **D10 (low, efficiency):** ghStructure's deep listings page folders first, and there is no size sort or top-N.
- **D11 (low):** a PR read on an issue number gives a generic `notFound` with no `next.readIssue`.
- **D12 (low):** artifactSearch reports a missing exact package as `empty` instead of `notFound`.
- **D13 (info):** an empty ghSearchCode result doesn't mention that the index only covers the default branch.
