# Octocode benchmarks

**Same answers, a fraction of the context, and no silent failures.** We ran Octocode head to head against the tools coding agents use today:
- GitHub's `gh` CLI, both as typically used and as expert `--jq` pipelines;
- `rg`, `sed`, `ast-grep` and `ctags` on local code.

The tasks were real research questions on real repositories and pull requests. **Every number on this page comes from a recorded run**, graded against answers verified independently of every tool. This is the single results document: headline, every table, what changed, where we lose, and how to reproduce it.

- [Headline](#headline)
- [1. Large pull requests](#1-large-pull-requests)
- [2. GitHub research against expert gh](#2-github-research-against-expert-gh)
- [3. Local research against rg and sed](#3-local-research-against-rg-and-sed)
- [4. Precision tools: LSP, AST, codemods](#4-precision-tools-lsp-ast-codemods)
- [5. clasify: judge before reading](#5-clasify-judge-before-reading)
- [6. Hostile inputs and safety](#6-hostile-inputs-and-safety)
- [7. Where plain tools still win](#7-where-plain-tools-still-win)
- [8. Hypotheses and verdicts](#8-hypotheses-and-verdicts)
- [9. How we got here](#9-how-we-got-here)
- [10. Method and reproduction](#10-method-and-reproduction)

## Headline

| Claim | Octocode | Best alternative | Result |
|---|--:|--:|---|
| Large-PR review, total context (4 PRs, 37–656 files) | **9.6k chars** | 45k (expert gh) · 15,590k (typical gh) | **4.7× less** than expert gh, **1,600× less** than typical gh. All answers correct |
| 656-file PR, "which build files switch to esbuild?" | **978 chars, 1 call** | 3.8k chars, 2 calls (expert gh) | `gh pr view` silently stops at 100 files, and `gh pr diff` fails above 300 |
| 22k-line-patch PR | **6.0k chars** | 40k (expert gh) · 7,269k (typical gh) | **6.7× less** than expert gh |
| GitHub research, fully correct (31 tasks) | **30/31** | 28/31 (expert gh) | gh reported 300 of 339 files as complete, missed a renamed repo, and gave unpinned reads |
| Symbol reference precision | **1.00** | 0.26–0.62 (`rg -w`) | No hits in comments, strings or unrelated names |
| Codemods identical to ast-grep | **5/5** | 1/5 (sed) | sed rewrote doc comments and missed multi-line calls |
| Secrets leaked into agent context | **0 of 5** | 3 of 3 (rg, ast-grep) | Every byte is scanned before it reaches the model |
| Reading a 3.2 MB file | **18k chars** first page | 3,151,772 (`cat`) | Bounded pages with honest totals |
| "How/where does X work?" (with clasify) | **10/10** | 9/10 (rg + sed) | Only approach to solve the Java cache question |

## 1. Large pull requests

**Task.** Each PR gets one specific review question, for example "which build files switch the bundler to esbuild?" or "which TCP tests stay disabled under Miri, and why?". An answer counts only if every expected file and line appears in text the agent read.

| PR | Size | Question |
|---|---|---|
| [microsoft/TypeScript#51387](https://github.com/microsoft/TypeScript/pull/51387) | 656 files | Which build files switch to esbuild? |
| [rust-lang/rust#157558](https://github.com/rust-lang/rust/pull/157558) | 429 files, renames | Which crates rename `errors` to `diagnostics`? |
| [microsoft/TypeScript#61986](https://github.com/microsoft/TypeScript/pull/61986) | 12 files, 22k changed lines | Where does `click` change from MouseEvent to PointerEvent? |
| [tokio-rs/tokio#8156](https://github.com/tokio-rs/tokio/pull/8156) | 37 files | Which tests stay ignored under Miri, and why? |

**Totals, all four PRs** (every approach answered 4/4):

| Approach | Context read | ≈ tokens | Calls | Time |
|---|--:|--:|--:|--:|
| gh, typical usage | 15,590k chars | 3,898k | 10 | 15 s |
| gh, expert `--jq` pipelines | 45k | 11k | 9 | 11 s |
| Octocode, step-by-step (summary → file list → patches) | 88k | 22k | 15 | 22 s |
| **Octocode, one filtered query per question** | **9.6k** | **2.4k** | **7** | **8.3 s** |
| Octocode + clasify scouting | 159k | 40k | 24 | 79 s |

**Per PR** (chars read / calls):

| PR | gh typical | gh expert | Octocode filtered | Octocode step-by-step | Octocode + clasify |
|---|--:|--:|--:|--:|--:|
| TypeScript#51387 | 6,854k / 3 | 3.8k / 2 | **978 / 1** | 36k / 4 | 74k / 7 |
| rust#157558 | 1,417k / 3 | **1.2k / 2** | 1.6k / 2 | 28k / 4 | 34k / 5 |
| TypeScript#61986 | 7,269k / 2 | 40k / 3 | **6.0k / 3** | 16k / 4 | 19k / 7 |
| tokio#8156 | 49k / 2 | **0.7k / 2** | 1.1k / 1 | 7.5k / 3 | 33k / 5 |

What the table shows:
- **Typical gh** reads whole diffs. On TypeScript#51387 it opened 414 files to answer a question about 2.
- **The filtered Octocode query** is the recommended flow, and the one the tool descriptions steer agents to. It asks the PR directly: `matchString` returns only matching hunks, `fileFilter` narrows by path, glob or status, and `matchContext: 0` returns just the changed lines.
- **On the two small PRs expert gh is a few hundred characters shorter.** The difference is Octocode's evidence: the head commit SHA and exact `@@` line numbers for every hit.
- **clasify scouting doesn't pay on PRs.** A literal filter settles the question 16× more cheaply, so the instructions tell agents to skip clasify here.

**Correctness beyond the answer** (36 checks on the same PRs, all passing):
- The file list matches GitHub's own (`gh api`) exactly: count, order, status and line counts.
- There are no duplicates or gaps across pages, and the head SHA stays stable.
- Patches are byte-identical to GitHub's, including a 100k-char patch read across windows.
- Renames keep the old path. The 192 files on TypeScript#51387 that GitHub sent without a patch are flagged `!tooLarge`/`!omitted`, not silently skipped.
- A PR over GitHub's 3000-file limit (rust#106458, 27,673 files) is reported as partial. It used to be reported as complete.

## 2. GitHub research against expert gh

**31 tasks** covered code search, repository search, trees, file reads, PR/issue/commit search, and reading PRs, commits and comparisons. The gh side used expert commands: `gh search … --json --jq`, `gh api --paginate`, and raw content with `sed`. Answers were checked with raw fetches, full recursive-tree diffs (786/786 files) and a blobless clone for the true compare count.

| | Fully correct | Context read | Calls | Time |
|---|--:|--:|--:|--:|
| gh, expert | 28/31 | 106k | 34 | 34 s |
| **Octocode** | **30/31** | 270k | 52 | 71 s |

**Per tool** (chars / calls):

| Tool | Tasks | gh expert | Octocode | Rating /10 |
|---|---|--:|--:|--:|
| Code search (`ghSearchCode`) | where code lives, phrase search, renamed repo, paging, owner-wide | 1,382 / 8 | 3,172 / 6 | 8 |
| Repository search (`ghSearchRepo`) | most-starred client, top repos of an org | 423 / 2 | 2,375 / 2 | 7 |
| Tree (`ghStructure`) | folder listings, a 786-file inventory, largest files | 27,460 / 4 | 34,269 / 13 | 6 |
| File read (`ghGetFileContent`) | function at a pinned SHA, a 1.45 MB file, a range plus its commit | 4,523 / 3 | 8,137 / 3 | 8 |
| History search (`ghSearchHistory`) | 38 merged PRs, open issues, file commits, a commit by keyword | 4,568 / 4 | 7,158 / 4 | 8 |
| History read (`ghGetHistoryItem`) | PR, commit, compare, issue thread | 66,593 / 8 | 211,343 / 19 | 6 |
| Package → repository (`artifactSearch`) | npm, PyPI, crates, monorepo, discovery | 874 / 5 | 3,818 / 5 | 8 |

**Where Octocode stays correct and gh fails silently:**

| Situation | gh (expert) | Octocode |
|---|---|---|
| Repository was renamed (facebook/react → react/react) | 0 results, no error | Follows the rename (`next.retryRenamed`) |
| Compare two releases (true count 339 files) | Returns 300 as complete | Flags GitHub's 300-file cap (`isPartial`, `providerFileLimit`) |
| PR with 656 files | `gh pr view --json files` stops at 100; `gh pr diff` fails with 406 | Filters all 656 in one call |
| Quoted phrase search | Returned 0 (gh re-quotes the query) | Correct on the first try |
| Read a file range | No commit recorded | Returns `commitSha`; later pages stay pinned to it |
| Private or missing repo in code search | Empty output, exit 0 | Typed error, non-zero exit |
| Rate limit burst | 403 with no reset time | Reports `retryAfterSeconds`; a local breaker stops doomed requests |
| Binary file | Raw bytes dumped into context | Detected, with a hint |
| 2.35 MB file | Must know not to read it | Partial first page, continuation pinned to the SHA |

Across all 31 tasks, gh's raw output is 2.6× smaller. Octocode's extra characters are its envelope: SHAs, pagination, next steps and flags. It still loses outright on whole-list dumps (see [section 7](#7-where-plain-tools-still-win)).

## 3. Local research against rg and sed

**Task.** 5 repositories (excalidraw/TSX, tokio/Rust, prometheus/Go, django/Python, guava/Java) × 3 questions each:
- **symbol:** find a definition and all its usages;
- **how:** how does X work? (the answer spans 2–3 files);
- **unknown:** where is this behavior implemented, when the symbol name is unknown.

**Totals:**

| Approach | Correct | Context read | Calls | Time | Files opened |
|---|--:|--:|--:|--:|--:|
| rg + sed (expert) | 14/15 | 105k | 30 | 0.9 s | 40 |
| Octocode | 14/15 | 113k | 25 | 2.3 s | 40 |
| Octocode + clasify (scout chunks) | 12/15 | 109k | 35 | 11 s | 24 |
| **Octocode + clasify (locate in files)**, how/unknown tasks | **10/10** | **97k** | 31 | 9.3 s | **18** |

**Per task** (chars read, ✓ = correct):

| Task | rg + sed | Octocode | + clasify (chunks) | + clasify (locate) |
|---|--:|--:|--:|--:|
| ts-symbol | 1.5k ✓ | 1.9k ✓ | 10.0k ✓ | — |
| ts-how | 6.2k ✓ | 7.4k ✓ | 7.5k ✗ | 8.2k ✓ |
| ts-unknown | 5.9k ✓ | 7.2k ✓ | **3.2k ✓** | 5.9k ✓ |
| rust-symbol | 1.7k ✓ | 2.0k ✓ | 6.6k ✓ | — |
| rust-how | 8.4k ✓ | 11k ✓ | **7.5k ✓** | 13k ✓ |
| rust-unknown | 9.6k ✓ | 10k ✓ | **6.6k ✓** | **6.1k ✓** |
| go-symbol | 0.2k ✓ | 0.7k ✓ | 5.5k ✓ | — |
| go-how | 10k ✓ | 11k ✓ | **8.5k ✓** | 11k ✓ |
| go-unknown | 11k ✓ | 10k ✓ | **7.5k ✓** | **5.9k ✓** |
| py-symbol | 0.5k ✓ | 1.1k ✓ | 5.3k ✓ | — |
| py-how | 12k ✓ | 14k ✓ | **10k ✓** | 19k ✓ |
| py-unknown | 9.4k ✓ | 11k ✓ | 7.6k ✗ | **8.2k ✓** |
| java-symbol | 1.1k ✓ | 1.2k ✓ | 6.7k ✓ | — |
| java-how | 14k **✗** | 12k **✗** | 11k **✗** | **12k ✓** |
| java-unknown | 13k ✓ | 12k ✓ | **5.7k ✓** | 8.4k ✓ |

What this shows:
- **Octocode matches expert rg + sed on accuracy (14/15).** It uses about 7% more characters, spent on citable line numbers, and fewer calls.
- **On "unknown" questions, clasify opens 1 file instead of 3** with the same answer, and usually with less context.
- **On java-how, only clasify locate found the answer.** The deciding line never matches a text search, so rg and plain Octocode both missed it.
- **For exact symbols, plain search is the right tool.** clasify adds cost there, and the instructions say to skip it.

## 4. Precision tools: LSP, AST, codemods

Across 8 repositories (TypeScript, TSX, Rust, Go, Python, Java, C, C++), every standard tool was used expertly, and every hit was graded by reading the code.

| Capability | Standard tool | Octocode | Rating /10 |
|---|---|---|--:|
| Text search (17 tasks, every rg flag family) | 17/17 exact | 17/17 exact | 7 |
| Range and match reads (22 tasks) | 22/22 byte-exact | 22/22 byte-exact | 7 |
| References: Rust `add_permits` | rg precision 0.62 | **1.00** (18/18) | 8 |
| References: TS `getNonDeletedElements` | rg precision 0.26 | **1.00** (7/7) | 8 |
| References: Python `slugify` | rg precision 0.31, and missed an aliased call | **1.00** (5/5) | 8 |
| Go to definition (TS, Python, Rust) | 2–4 rg calls, manual disambiguation | **1 call, exact** | 8 |
| Declaration inventory (6 files) | regex/ctags precision 0.81–0.98 | **6/6 exact** | 8 |
| Codemods (5 across Rust, TS, Go, Python) | sed wrong on 4/5 (rewrote comments, missed multi-line) | **5/5 identical to ast-grep** | 7 |
| Rewrite after the file changed | ast-grep applies anyway | **Blocked**, file untouched | — |

## 5. clasify: judge before reading

`clasify` (optional, needs a classification key) judges **unread** candidates:
- **Scout** rates each candidate file or item.
- **Locate** finds the line window that answers a question.
- **Sufficient** checks whether a snippet already answers it.

It returns verdicts and windows, never file bodies.

**Live test: one GitHub research task, 10 questions, 14 provider calls, 4.0 s.**

| Measure | Result |
|---|---|
| Right file ranked first (`bounded.rs` 0.85 vs 0.44 and 0.35) | P@1 = 1.0; 2 of 3 reads pruned |
| PR history, relevant items on top (10 items) | Everything above 0.5 was relevant; 7 of 10 reads pruned |
| Locate: exact line inside the top window | **4/4** (e.g. permit release at `bounded.rs:1738`, acquire at `:1302`) |
| Context read: reading every candidate vs following clasify's picks | 160,308 → **30,656 chars (81% less)** |
| Payload audit, 14 captured calls | Evidence, goal, reasoning and question sent; **no tokens, cursors or snapshots** |
| Cache | Identical repeat: **0 provider calls**. A changed question, goal or content triggers a new call. Errors never cached |

**The rule the benchmarks produced:**
- **Use clasify** for a behavioral target (no exact literal) across two or more candidate files.
- **Skip it** for identifiers, literals and PR filters.
- **Without a key** the tool disappears cleanly: no tool, no mentions, no dangling next steps.

## 6. Hostile inputs and safety

| Input | Standard tool | Octocode |
|---|---|---|
| Fake secrets in a repo (AWS key, GitHub token, DB URL, RSA key) | rg and ast-grep printed all 3 checked values | **0 of 5 leaked**; redacted with a warning |
| 3.2 MB source file, whole read | `cat`: 3,151,772 chars | **18,355-char first page** plus a continuation |
| Same file, identifier with 166 hits | 16,530 chars | 2,741-char first page, reports "166 total" |
| 2 MB single-line minified bundle | naive `rg`: 2,077,782 chars | **710 chars**, clipped and flagged |
| Symlink to `/etc/hosts`, or a `../` escape | `rg -L` follows and prints it | **Refused** |
| `.env` file | Readable | **Refused** |
| Binary file | Raw bytes | **Detected**, with a hint |
| Invalid regex | Parse error | `invalidRegex` plus a literal-search repair |
| Zero results | Silent exit 1 | Typed "empty" plus scope-repair hints |

## 7. Where plain tools still win

| Area | Numbers |
|---|---|
| **Per-call latency** | rg: 3–25 ms; Octocode CLI: about 200 ms per call (the MCP server stays warm). Language-server cold starts take 3–7 s |
| **Whole-list GitHub dumps** | Full compare: gh 30k / 1 call vs Octocode 146k / 8. Deep tree: 1 call vs 10. Issue thread: 1 call vs 3 |
| **Tiny answers** | On one-line PR answers, gh is 300–500 chars shorter; the difference is SHAs and line numbers |
| **Plain text search** | About 7% more characters than expert rg, spent on citable line numbers |
| **Output weight** | Repository search rows cost 6× gh's projected fields; rewrite previews cost 4.9× ast-grep's |

## 8. Hypotheses and verdicts

| # | Hypothesis | Verdict |
|---|---|---|
| H1 | On large PRs, Octocode needs less context than an expert gh user | **Supported**: 9.6k vs 45k |
| H2 | A naive step-by-step PR flow is competitive | **Rejected**: 88k vs 45k. Agents are now steered to the filtered query |
| H3 | Locally, Octocode matches rg/sed accuracy | **Supported**: 14/15 each, at 7% more context |
| H4 | clasify raises accuracy on behavioral questions | **Supported**: 10/10 vs 9/10; solves java-how |
| H5 | clasify helps everywhere | **Rejected**: it costs 16× more on PR filters and adds cost on exact symbols |
| H6 | Octocode degrades cleanly without a clasify key | **Supported** |
| H7 | clasify's provider payload is complete and safe | **Supported**: no leaks across 14 calls |
| H8 | Octocode is safer than raw tools on hostile input | **Supported**: 0/5 secrets leaked vs 3/3 |
| H9 | Octocode's GitHub tools give signals gh doesn't | **Supported**: rename, caps, patch flags, pinning, rate limits |
| H10 | Documented features behave as documented | **Mostly**: 167 of 217 claims verified live, 35 partial, 14 failed (all fixed or documented) |

## 9. How we got here

The measurements drove the product, round by round. PR and local totals are from the benchmark suite.

| Round | Change | PR, Octocode | Local, Octocode |
|---|---|--:|--:|
| Baseline | — | 247k (step-by-step) | 139k · 13/15 |
| 1 | Compact PR file list, `fileFilter`, matching hunks only, verbatim patches; ranked hits inside files | 10k (filtered) | 121k · 14/15 |
| 2 | `matchContext`, trimmed patch headers and menus; clasify cluster coverage | 10k | 121k |
| 3 | Leaner line gutter, no single-row wrappers, declaring file ranked first, instructions ≤ 2,000 chars | **9.6k** | **113k** · 14/15 |

Across the same work:
- Minimal-by-default responses cut output 16% with no data lost.
- Peak search threads went from 72 to 22.
- About 40 defects found by the validation suites were fixed with tests.

## 10. Method and reproduction

- **Same question, same answer key.** Every approach answers the same question and is graded against the same ground truth, verified independently: `git grep`, ast-grep, reading the code for each reference hit, `gh api`, and a blobless clone for true file counts.
- **Experts on both sides.** gh and the shell tools got expert flags; Octocode got the equivalent expert query.
- **What we counted.** Characters the agent reads (≈ tokens × 4) across every call and continuation, plus wall-clock time per task.
- **Environment.** Octocode 19.2 (native 20.0), gh 2.96, ripgrep 14.1, ast-grep 0.45, rust-analyzer 1.96, clangd 21, macOS arm64. Runs dated 2026-09-29/30. Benchmark results are from round 3.

```bash
node octocode-local-testing/bench/bench.mjs --suite all --label mine   # PR + local suites
node octocode-local-testing/harness/pr-large-bench.mjs mine            # large-PR correctness (36 checks)
```

- [BENCHMARK.md](../octocode-local-testing/bench/BENCHMARK.md): instructions, tasks, fairness rules.
- [repos/README.md](../octocode-local-testing/repos/README.md): test repositories at pinned commits.
- Raw validation reports: `octocode-local-testing/bench/validate/{github,local,features,orangu,release}/REPORT.md`.
- [OCTOCODE_PROTOCOL.md](OCTOCODE_PROTOCOL.md): why these results happen.
