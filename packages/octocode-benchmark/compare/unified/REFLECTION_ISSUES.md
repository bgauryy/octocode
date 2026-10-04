# Benchmark reflections: octocode issues to fix

This file collects the octocode problems that come up again and again in worker reflections across every run under [`results/`](results/). Each issue cites quotes and scores from the runs.

The prompt for each reflection is in [REFLECT.md](REFLECT.md). We read 731 `reflection.md` files from 10 runs: full-1, smoke-1, dedup-20260930, production-review-20260930, tri-20261002, s12-p1/p2/p3, s12fix-p1/p2 and s13-screen. dedup-fix, probe-diag, smoke-2 and the tri-dryprobe2 runs have no reflections. For production-review-fixed-v3 we used only its REPORT.md.

Counts are approximate keyword and manual tallies made by three reviewers. Treat them as relative weights, not exact numbers.

## Headline

Octocode's quality is about equal to rg-gh's in later runs, but it costs **1.5–1.8× the tokens**. In early runs the cost was 2.5–6.8×.

Three things explain most of the gap:

1. **Evidence comes back with holes.** Multi-range and `matchString` reads show "lines omitted". PR patches are cut with `...`. Paginated pages are never followed.
2. **Line numbers are missing or ambiguous.** Without them the worker cannot write exact `path:line` citations, and that costs points on the judge's evidence score.
3. **Fixed context is high.** octocode starts at about 9.2–9.8k tokens and octocode-npm at 36.4k, against 4.6–4.8k for rg-gh.

The headline issues (#1 and #2) are failures of the repository's own **"never trim, always paginate"** rule (AGENTS.md).

## Results at a glance

| run | octocode Q | rg-gh Q | tokens octo / rg | ratio | W/T/L (octo vs rg) |
|---|--:|--:|--:|--:|---|
| full-1 | 8.42 | 9.12 | 5,697k / 1,050k | 6.08× | — (34 octo tool errors) |
| smoke-1 | 8.25 | 9.00 | 258k / 38k | 6.84× | 2 Q |
| dedup-20260930 | 8.38 | 9.18 | 2,475k / 1,088k | 2.53× | 3/11/16 |
| production-review-fixed-v3 | 8.38 | 8.97 | 2,361k / 941k | 2.70× | 3/14/13 |
| tri-20261002 | 8.99 (npm 8.80) | 8.93 | 1,672k (npm 5,195k) / 919k | 1.93× (npm 6.14×) | 6/19/5 |
| s12-p1 | 8.97 | 9.16 | 2,312k / 1,577k | 1.64× | 8/28/13 |
| s12-p2 | 9.00 | 8.88 | 2,262k / 1,666k | 1.52× | 9/33/7 |
| s12-p3 | 9.09 | 9.11 | 2,267k / 1,489k | 1.58× | 9/28/12 |
| s12fix-p1 | 9.11 | 8.92 | 789k / 508k | 1.66× | 6/9/3 |
| s12fix-p2 | 8.86 | 8.86 | 810k / 500k | 1.81× | 4/11/3 |

s13-screen ran without a judge, so it has token data only (28 shared questions):

| worker | tokens | calls | first-request context | errors |
|---|--:|--:|--:|--:|
| octocode | 1,364k | 85 | 9.2k | 0 |
| octocode-flat | 1,402k | 105 | 8.7k | 2 |
| octocode-defer | 1,365k | 87 | 9.0k | 1 |
| **octocode-family** | **1,072k** | 88 | **6.3k** | 1 |
| octocode-guide | 1,495k | 100 | 9.4k | 5 |
| rg-gh | 946k | 95 | 4.8k | 7 |

Some questions lose again and again: **G10** (6–7.5 vs 9–10, a 37-file PR where page 2 was never fetched), **G01**, **G02**, **G06**, **G12**, **L09**, **L13** and **L24**.

## Issues, ranked by frequency × impact

### P0-1 Elision inside fetched content: "lines omitted"

- **Tools:** `localFetch` (multi-range, `matchString`) and `ghGetFileContent` (`matchString`).
- **Frequency:** about 140 reflections, in every run and every s13 variant. That is roughly 50 in s13, 41 in s12-p1/p2/fix-p1, 25 in s12-p3/fix-p2 and 20 in early runs.
- **Evidence:**
  - "The 255-275 range was elided inside my own fetch" (s13 L16/octocode-guide)
  - "wide range (165-360) was truncated, so lines 169-359 were omitted" (s13 L04/octocode-guide)
  - "elided large middle sections… so I never saw the enclosing function signatures… My final answer still named them" (s12-p1 L24/octocode, 7.0 vs 9.0)
  - "fetch silently omitted lines 681–699 and 761–803" (tri L15/octocode)
- **Impact:**
  - Citations become approximate ("~425-464", s13 L11).
  - The worker needs extra follow-up calls. In s12-p1, L26 used 80.5k tokens vs rg-gh's 21.0k.
  - Answers name symbols the worker never actually saw.
- **Fix:**
  - Never elide inside a span the caller requested explicitly. If a page budget applies, return `next` for exactly the rest of that span.
  - Print gaps *between* requested ranges as a single short marker that includes a ready-to-run fetch.
  - Merge overlapping or nearby windows.
  - For `matchString`, offer whole-enclosing-block mode (function or class) and not only N-line windows.

### P0-2 PR patches: `...`-minified, partial, no new-side line numbers

- **Tool:** `ghGetHistoryItem` (patches).
- **Frequency:** about 90 reflections, in every run with GitHub questions.
- **Evidence:**
  - "patch view is condensed, with `...` elisions and no absolute line numbers. I couldn't give `path:line`" (dedup G04)
  - "came back minified, with elided hunks and `partial: true`… never followed `next.continuePatch`" (s12fix-p1 G01)
  - "`tests/test_arguments.py` was cut off mid-parametrize" (s12fix-p2 G04)
  - "Hunk-only patches don't show test names" (s13 G10/octocode-family)
- **Impact:** this issue drives the worst losses:
  - G10: 6 vs 10 (86k vs 26k tokens).
  - G01: 7 vs 9.
  - G03 and G04: 7.5.
  - fixed-v3 G10: 6.5 vs 9.5.
- **Fix:**
  - Keep `@@ -a,b +c,d @@` headers along with the enclosing symbol, and give new-side line numbers.
  - Make minify `none` the default for code hunks, at least when the patch is small.
  - For PRs with more than N files, return a compact per-file summary first, with patches paged per file.
  - Show the counts of unread files and patches at the top of the response.
  - Allow one call to fetch both patches and the merged file. Today the worker skips `readAtMerge` and `continuePatch`; about 21 reflections admit this.

### P0-3 Line numbers missing or inconsistent

- **Tools:** `localFetch`, `localGetFileContent` (`fullContent`) and `ghGetFileContent` (`fullContent`) return no line numbers. `localSearch` `detailed` returns a `line` field that disagrees with its content.
- **Frequency:** about 28 reflections with no numbers (early runs and tri), plus about 15 with mismatches (tri, s12, s13).
- **Evidence:**
  - "`localFetch` returned line ranges but didn't prefix each line with its number. I had to count offsets" (full-1 L12)
  - "`fullContent` responses carried no line numbers" (tri G07/octocode-npm)
  - "the match `line` field (346) disagreed with the content's first line number (338)" (s13 L11/octocode-defer)
  - "returned `src/db.c` line numbers that were off from the content" (tri L13)
  - "`localSearch` snippets were sometimes scrambled" (tri L08)
- **Fix:**
  - Number every content line by default, using a compact gutter.
  - Make the snippet and the `line` field share one source of truth.
  - Fix how overlapping context windows are assembled. The scrambled order looks like a bug.

### P0-4 Pagination and `next.*` not followed

- **Tools:** `ghGetFileContent` (batched files), `ghGetHistoryItem`, `localSearch` ("+12 more").
- **Frequency:** about 30 reflections.
- **Evidence:**
  - "batched three files in one `ghGetFileContent` call. The response was paginated at the response level, so only `_runtime.py` came back" (full-1 G01, repeated in s13 G01)
  - G10: 37 files, page 2 never fetched (s12-p1, s12-p3, s12fix-p2).
- **Fix:**
  - Split the response window fairly across batched rows, so every row returns something.
  - Show an `incomplete: N rows / M files unread` banner at the top of the response, not only in metadata.
  - Rank the `next` continuations first.

### P1-5 No way to confirm the pinned commit

- **Tools:** all local tools, plus history tools for ancestry.
- **Frequency:** about 100 reflections: s13 ≈32, s12/s12fix ≈55, tri/early ≈16.
- **Evidence:**
  - "No tool I called reported the checked-out SHA" (tri L14)
  - "The directory isn't a git repo" (s13 L14)
  - "The tools gave me no direct ancestry check" (s12fix-p1 G16)
- **Fix:**
  - Report the HEAD SHA (when the directory is a git repo) in `localSearch`, `localFetch` and `structureSearch` metadata.
  - Add a commit-ancestry operation (GitHub compare) under `ghGetHistoryItem`, and hint it on history questions.

### P1-6 `ghSearchCode` ignores the pinned ref

- **Tool:** `ghSearchCode`. It searches the default branch only.
- **Frequency:** about 30 reflections (s12 G07/G11/G12/G13/G14/G18, s12fix, s13, tri).
- **Evidence:**
  - "It returned commit 0ecb5c2, not v1.20.0, and its line numbers (271, 574) differ from the tag's" (s12-p3 G14)
  - "Its hint also pointed at the wrong SHA" (s12fix-p1 G12)
- **Impact:** G12 scored 8 vs 10 (s12-p1/p2). rg-gh got this right with a tree walk plus `grep -n` at the ref.
- **Fix:**
  - When a ref or pin is known and differs from the default branch, return a top-level warning plus `next` → `ghGetFileContent`/`matchString` at the ref.
  - Flag results where `commitSha ≠ requested ref`.
  - Consider a "grep files of a tree at ref" mode.
  - On an empty path-scoped result, suggest dropping the path.

### P1-7 Noisy `localSearch` output

- **Frequency:** about 110 reflections (s13 ≈43, s12/s12fix ≈51, tri/early ≈17).
- **Causes:**
  - Broad regexes return about 100 hits.
  - Comments, docstrings and Javadoc hits.
  - Generated files ("most hits were generated `commands.def` lines", s12-p2 L28).
  - Duplicate sync/async or vendored copies.
  - `contextLines` 25–60 dumps unrelated code.
  - The `binarySkipped` count shows up as noise ("1226 skipped `.mo` binaries", s13 L02).
- **Fix:**
  - Rank definitions above comments and down-weight generated files.
  - Group hits by enclosing symbol and collapse duplicates.
  - Show the match count per regex alternative.
  - Surface the binary-skip count only when it affects completeness.

### P1-8 Semantic tools never used (`lspSearch`, `astSearch`)

- **Frequency:** 0–1 calls per run. Yet about 100 reflections say "next time use `lspSearch`" for callers, references or the enclosing function.
- **Evidence:**
  - "The text search can't tell callers of the getter from mentions of the option field" (s12-p1 L20)
  - "I called the line 864 caller `save_base` without seeing its `def` line" (s13 L02/octocode-defer)
- **Other problems:**
  - The `astSearch` name filter looked ignored and returned 93–108 declarations (s12 L23).
  - `lspSearch` failed without `lineHint` (s12-p3 L23).
- **Fix:**
  - Include the enclosing symbol name in each `localSearch` hit. This removes most of the need for `lspSearch`.
  - When the pattern looks like a symbol, emit a ready-to-run `lspSearch` references or callers `next` with `lineHint` pre-filled.

### P1-9 Schema and input friction (first-call validation failures)

- **Frequency:** about 60 cases across runs. In full-1 these caused 34 errors, mostly from the required `goal`/`reasoning` fields, which have since been relaxed.
- **Recurring cases:**
  - Arrays passed as strings, such as `excludeDir: "[...]"`: about 14.
  - Missing `queries` wrapper: about 7.
  - `ranges` written as `"a,b"`, nested arrays, a bare `"1084"`, or a JS expression: about 12.
  - `ranges` combined with `matchString` is rejected (s12-p1 L13, tri L09).
  - `contextLines` above 100 is clamped with no warning or no follow-up (G05 in every s12 run).
  - Unknown `regex` values such as `"rust"` or `"literal"` are accepted silently.
  - `()` and `[` are parsed as regex (s13 L02, L09).
  - `include` globs with commas return empty (s13 L04).
  - One bad row discards the whole batch (tri L09).
- **Fix:**
  - Coerce a scalar to an array.
  - Accept `a,b`, `[a,b]` and a bare `n` as ranges.
  - Allow `ranges` together with `matchString`.
  - When clamping, add a warning plus a `next` that covers the rest of the window.
  - Reject unknown enum values loudly.
  - When a search returns zero hits and the pattern contains regex metacharacters, retry it as a literal automatically.
  - Fail individual rows, not the whole batch.

### P2-10 Path problems

- Returned paths don't match the repo layout ("`typescript/tsc/internal/…`", dedup L19); about 6 reflections.
- Guessed paths lead to `pathNotFound` or 404; about 8 reflections.
- **Fix:**
  - Return paths relative to the caller's `path`.
  - On `pathNotFound`, return the nearest existing paths (fuzzy match).

### P2-11 Oversized or unreadable outputs

- `ghGetHistoryItem` with `patches: all` returned about 64K characters. The output spilled to a file that `localGetFileContent` refused to read (tri G02/octocode-npm).
- `structureSearch` returned about 20k characters that the worker never used (tri L20), and it was "nearly useless" with withheld entries (s12-p2 L20).
- `artifactSearch` gives a dependency count without names (G17 in every s12 run).
- **Fix:**
  - Make spill files readable.
  - Lower the `structureSearch` default caps.
  - Have `artifactSearch` list dependency names, paged.

### P2-12 Fixed context and tool surface

- The first-request context is 9.2–9.8k tokens for octocode and 36.4k for octocode-npm, against 4.6–4.8k for rg-gh.
- In s13, **family** (6 local or 7 GitHub tools per question) cut tokens by 21% and context by 2.9k, with no new complaints. It also stopped cross-family drift. In guide L14, by contrast, the worker spent 4 calls on GitHub tools for a local question.
- **guide** was the worst variant: +10% tokens and 5 errors. Workers quote its rules and then break them, so the instructions do not change behavior.
- **defer** added nothing (`run` was never called). **flat** was neutral (+3%).
- **Fix:**
  - Promote per-question tool families to the default, and confirm with a judged run.
  - Drop the guide instructions.
  - Investigate the 36k context of octocode-npm.

### P2-13 Harness issues (not tool issues)

- In production-review-20260930, 5 octocode runs (G03–G07) saw **no MCP tools** ("no tool definitions were visible to me"). The harness should fail fast when 0 tools are registered.
- rg-gh was penalized by its environment: `rg` was not installed (about 19 reflections), sandbox `/dev/null` and `/tmp` writes were blocked, and `git log` broke. This makes the baseline look weaker than it is.
- s13 has no judge pass, and s12 / s12fix-p2 have "isolation probe not run".

## Keep: what helped

| strength | approx. mentions |
|---|--:|
| One `localSearch` regex alternation maps a whole mechanism with line numbers | ~100+ (L questions) |
| Batched multi-range `localFetch` / `ghGetFileContent` at the pinned SHA | ~120 |
| `ghGetHistoryItem` returns body, `changedFiles`, merge SHA, `closedBy` and selected patches in one call | ~90 |
| `matchString` anchored reads with `matchedLines` | ~85 |
| `changedFiles` / file filter, which avoids paging large PRs | ~18 |
| `hints.read` / `next` pointing at the right file (when followed) | ~37 |
| Resolved full `commitSha` returned even from a short SHA | several |
| `ghSearchHistory` for history questions (s12fix-p2 G15 scored 10 vs 7) | several |
| `searchLiteral` hint on empty regex results (s13 L02) | few |

## Where rg-gh did better (and octocode should match)

1. **No gaps.** `sed -n 'a,bp;c,dp'` and `awk '/def a/,/def b/'` return every requested line.
2. **Line numbers everywhere.** `rg -n`, `grep -n` and `cat -n` support exact citations.
3. **Complete diffs, indexed locally.** `gh pr diff > f`, then `grep -n '^diff'`, then jump to the parts that matter. s12fix-p2 G10 scored 10 vs 6.
4. **Grep at a ref.** List the tree at the ref, then loop files with `grep -n` (G12). Read exact files with `gh api contents?ref=SHA`.
5. **Commit check.** `git rev-parse HEAD`, plus tag-to-SHA resolution via `git/ref/tags`.
6. **Composition.** One shell call chains search and several reads.
7. **Compact timelines.** `gh api search/issues --jq` prints one line per PR (G15, G16).
8. **Small fixed context:** 4.6–4.8k vs 9–10k for octocode.

## Suggested order of work

1. **P0-1 + P0-3.** Remove elision inside requested spans, add line numbers everywhere, and fix the snippet/line mismatch. This is the biggest win for both quality and tokens, and it is a direct no-trim rule fix.
2. **P0-2 + P0-4.** Patch hunks with new-side line numbers, a per-file summary for large PRs, fair per-row pagination and an "incomplete" banner.
3. **P1-5 + P1-6.** Report the HEAD SHA, add an ancestry check, and add a ref guard on `ghSearchCode`.
4. **P1-8.** Add the enclosing symbol to `localSearch` hits and ready-to-run `lspSearch` `next` calls.
5. **P1-9.** Make input validation lenient and add per-row failure.
6. **P2-12.** Run a judged run with per-question tool families as the default. Also fix the harness issues (P2-13) so the baseline is fair.

The raw per-run reviewer reports are under `.octocode/tmp/agents/researcher-05ebb6/`, `researcher-e7e488/` and `researcher-93e0b0/` (`report.md`).
