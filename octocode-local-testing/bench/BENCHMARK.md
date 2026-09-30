# Octocode research benchmark

## Purpose

This benchmark measures how much context an agent must read to answer a research question correctly. It compares four approaches:

- **octocode** (MCP tools, no clasify)
- **octocode + clasify** (a semantic scout decides what to read)
- **gh** (typical usage, and lean usage with expert `--jq` flags)
- **rg / find / sed** (expert shell usage)

It covers GitHub PR review on 4 PRs and local code research on 5 repos (15 tasks). Each task has ground truth fixed in advance. Every number comes from a real run. Nothing is estimated.

Files:

| file | role |
|---|---|
| `bench.mjs` | single entry point: runs the arms, judges them, writes the results |
| `tasks.mjs` | PR and local tasks, the arm inputs, and the ground truth |
| `ground-truth.json` | frozen usage lists for the symbol tasks, plus each repo's commit |
| `results/bench-<label>.json` | full per-step record: every call, its chars and ms, and what was found |
| `results/bench-<label>.md` | tables |

It reuses `../harness/mcp-client.mjs`: `call()` adds `goal`/`reasoning`, and `raw()` replays `next.*` continuations verbatim.

## Prerequisites

1. **Built octocode.** The bench spawns `packages/octocode-mcp/dist/index.js` and never builds anything itself. Build it from the repo root first. See the root README; the fast path is `yarn build:dev`.
   - The MCP refuses to start while the core and native contract fingerprints differ, for example during a rebuild. The bench probes the MCP first and retries every 30 s for up to `--wait-mcp <min>` (default 20).
2. **Local repos.** They live under `octocode-local-testing/repos/{tsx,rust,go,python,java}`, cloned by the local-testing setup. Their commits are pinned in `ground-truth.json`. When a repo moves, `gtProblems` reports it; re-freeze and re-check the markers.
3. **`gh` authenticated** (`gh auth status`). The PR suite makes about 60 GitHub API calls per run.
4. **`rg`** on PATH, or `RG_BIN=/path/to/rg`. When neither is found, the bench falls back to the ripgrep bundled with Claude Code (`ARGV0=rg ~/.local/bin/claude`).
5. **clasify key.** The octocode+clasify arms need the classification API key from the user's octocode config (the same one `octocode clasify` uses). When clasify is unavailable, those arms record `unavailable` and the error, and no numbers are made up for them.
6. Env: `ENABLE_LOCAL=true` is set by the bench. GitHub auth comes from the normal octocode token resolution (`gh` token or env).

## Running

```bash
node octocode-local-testing/bench/bench.mjs --suite all   --label baseline
node octocode-local-testing/bench/bench.mjs --suite pr    --label pr-only
node octocode-local-testing/bench/bench.mjs --suite local --label local-only
node octocode-local-testing/bench/bench.mjs --suite local --only rust-how,go-how --arms octocode,octocode+clasify --label spot
node octocode-local-testing/bench/bench.mjs --freeze-gt        # re-freeze symbol usage lists after a repo update
```

Runtime is about 3–6 min for `all`, dominated by GitHub and clasify latency. Compare two labels by diffing `results/bench-*.md`. The JSON keeps every step, so you can tell which call regressed.

## Metrics and fairness rules

| metric | definition |
|---|---|
| chars | Total characters the agent receives: shell stdout+stderr, or the MCP `content[].text`. This is what the model reads. |
| ≈tokens | chars / 4 |
| calls | Tool invocations. One shell command line counts as one call, even when it loops over files, just as one batched MCP call with several `queries[]` counts as one. |
| ms | Wall time summed over calls. It includes network and clasify provider latency. |
| recall | The share of ground-truth files whose marker lines all appeared in text the arm saw for that file. |
| read P | Read precision: GT files among the files whose body (patch, file window, or source) the arm pulled in. It measures wasted reads. |
| files read | The number of files whose body entered context |
| usages R / P | Symbol tasks only: the usage-file list the arm produced, compared with the frozen `rg -l -w` list |
| correct | recall = 1, plus usage R = P = 1 for symbol tasks |

Fairness rules:

- Every arm gets the same task, the same question, and the same ground truth.
- Discovery inputs (search regex, keyword, candidate path filter) are identical across arms. They come from the wording of the task, never from the answer.
- "gh lean" and "rg/sed" are what an expert would type: `--jq` projections, TSV, `-l`, `-n`, and bounded `sed -n a,bp` windows.
- Reading policy is mechanical and the same in shape for every arm (see below). Nothing is hand-picked after seeing the answer.
- The count is what the agent reads. Work that clasify does on the provider side is not agent context, but its latency is included in ms.
- An arm "answers" only from text it actually saw. The judge is the ground-truth marker check applied to that text, so answer precision is 1 by construction. The real precision cost shows up as **read P** and chars.

## Arms and exact commands

### PR suite

`{r}` is `owner/repo`, `{n}` is the PR number, and `{sha}` is the head SHA.

| arm | steps |
|---|---|
| gh-typical | `gh pr view {n} -R {r}`, then `gh pr diff {n} -R {r}`. If the diff fails (GitHub refuses diffs of more than 300 files or 20k lines), `gh api --paginate repos/{r}/pulls/{n}/files?per_page=100` (raw JSON). For a task-relevant file without a patch that is still unseen: `gh api repos/{r}/contents/{f}?ref={sha} -H 'Accept: application/vnd.github.raw'` (whole file). |
| gh-lean | `gh pr view --json title,state,headRefOid,changedFiles,additions,deletions`, then one `gh api --paginate …/files --jq` that selects files matching the task regex **or** having no patch, and prints `### path\tstatus\tprevious\tNO_PATCH` plus only the matching `-`/`+` lines. Task-relevant files without a patch go through `gh api …contents… raw \| grep -n -F <literal>`. |
| octocode | `ghGetHistoryItem` metadata, then the inventory (`content.changedFiles`, `pageSize:100`, following `next…ChangedFilesPage`). Candidates = task path filter. Then `content.patches.mode:"selected"` for the candidates plus `matchString:<keyword>`, following `continuePatch`. Files without a patch get `ghGetFileContent` with `matchString` at `sourceSha`. |
| octocode+clasify | Same metadata and inventory, then **one clasify resource per candidate file** (`ghGetHistoryItem` selected patch for that file). A matrix holds at most 12 resources and one call carries up to 5 matrices. Each resource captures at least 2 pages, and a large patch captures more. When a matrix returns `classificationExpandedCellsExceeded`, the arm follows the hint: it splits the resources in half and retries. The failed call's chars and ms still count. The question is `contribution:<semantic>`. Then selected patches (no keyword) for files with max page P ≥ 0.5. A file without a patch goes to clasify `locate` on `ghGetFileContent fullContent` with `prefilter`, and then the best window ±5 lines is read. |

### Local suite

| arm | symbol task | how / unknown task |
|---|---|---|
| rg/sed | `rg -n --no-heading -e <defPattern> <path>` and `rg -l -w <symbol> <path>` | `rg -n [-i] [--glob '!<exclude>'] -e <pattern> <path>`, then for the top 3 files by hit count, `sed -n a,bp` over the densest 60-line hit window (one shell line) |
| octocode | one `localSearch` call with 2 queries: `searchText:<defPattern>`, plus `searchText:<symbol>, wholeWord, resultView:"files"` | `localSearch` (same pattern, `caseMode`, `exclude`), then one batched `localFetch` of the top 3 files' densest 60-line window over the returned hit lines (shown rows plus a clipped file's `pagination.moreLines`) |
| octocode+clasify | the usage list via `localSearch resultView:"files"`, then clasify `locate "Where is <symbol> declared"` over `localSearch{symbol} candidateEvidence:"fileChunks"`, then `localFetch` of the window | clasify `locate` (one question per sub-question of the task) over the **unread** `localSearch` resource with `candidateEvidence:"fileChunks"`. Reads follow the documented rule: `best` entries with `exists ≥ 0.5` (at most 2 per question, read at the entry's `path`), or else the top page match. Each window ±8 lines goes through one batched `localFetch`. |
| octocode+clasify(files) | n/a | The same `localSearch` as the octocode arm (output **seen**). Then clasify `locate` over the top 3 hit files as `localFetch {fullContent, minify:"none"}` resources, with `prefilter` = the distinct matched strings from the search output (derived mechanically). On `classificationExpandedCellsExceeded`, it retries with one matrix per question in the same call. Then it reads `best` (`exists ≥ 0.5`, at most 2 per question, else the top entry) ±8 lines. |

## Tasks and ground truth

Ground truth was verified before the run: via `gh api …/files` and raw file diffs for PRs, and via `rg` for the local repos. Every run re-verifies it and records any drift in `gtProblems`.

### PR tasks

| id | PR / shape | review task | ground truth: files and required marker lines |
|---|---|---|---|
| ts-51387 | microsoft/TypeScript#51387: 656 files; the biggest files have no patch | Which build files switch the bundler to esbuild? Show the lines that add, import, or invoke it. | `Herebyfile.mjs` (`import esbuild from "esbuild";`, `esbuild.build(`), `package.json` (`"esbuild": "^0.15.13"`) |
| rust-157558 | rust-lang/rust#157558: 429-file rollup with 7 renames | Which crates rename `errors` to `diagnostics`? Old and new paths, plus the `mod` change. | 3 renames (`rustc_ast_lowering`, `rustc_ast_passes`, `rustc_builtin_macros`: `src/errors.rs` → `src/diagnostics.rs`; marker = old path) and 3 `src/lib.rs` (`+mod diagnostics;`) |
| ts-61986 | microsoft/TypeScript#61986: 12 files, 22k lines; `dom.generated.d.ts` has no patch | Which files show `click` going from MouseEvent to PointerEvent? Include the lib declaration. | `src/lib/dom.generated.d.ts` (`"click": PointerEvent`, checked by diffing raw base and head), `correlatedUnions.js` and `.types` (`callback: (ev: PointerEvent) => void`), `reverseMappedTypeContextualTypesPerElementOfTupleConstraint.types` (`listener: (event: PointerEvent) => void`) |
| tokio-8156 | tokio-rs/tokio#8156: 37 files (medium) | Which tests stay disabled under Miri with a stated reason? | `tcp_shutdown.rs`, `tcp_socket.rs` (2 distinct reasons), `tcp_stream.rs`: the `cfg_attr(miri, ignore = "…")` lines |

### Local tasks

Each repo has 3 tasks: symbol, how, and unknown.

| id | repo | question | discovery input | ground truth (file: marker) |
|---|---|---|---|---|
| ts-symbol | tsx (excalidraw) | def and usages of `newElementWith` | `export const newElementWith\b` / `newElementWith` | `packages/element/src/mutateElement.ts: export const newElementWith = …`; 32 usage files |
| ts-how | tsx | how undo applies a history entry | `applyTo\(` (tests excluded) | `packages/excalidraw/history.ts: historyDelta.applyTo(`; `packages/element/src/store.ts: delta.elements.applyTo(` |
| ts-unknown | tsx | snap distance in px | `-i snap.?(distance\|threshold)` | `packages/excalidraw/snapping.ts: const SNAP_DISTANCE = 8;` |
| rust-symbol | tokio | free fn `spawn_blocking` (7 same-named defs) | `pub fn spawn_blocking<` / `spawn_blocking` | `tokio/src/task/blocking.rs: pub fn spawn_blocking<F, R>(f: F) -> JoinHandle<R>`; 35 usage files |
| rust-how | tokio | mpsc backpressure: wait and release | `acquire\|add_permit` | `bounded.rs: semaphore.acquire(n).await`; `chan.rs: self.inner.semaphore.add_permit();` |
| rust-unknown | tokio | default blocking-pool thread cap | `-i blocking.?threads` | `tokio/src/runtime/builder.rs: max_blocking_threads: 512` |
| go-symbol | prometheus | `NewEngine` | `^func NewEngine\(` / `NewEngine` | `promql/engine.go: func NewEngine(opts EngineOpts) *Engine {`; 6 usage files |
| go-how | prometheus | sample_limit enforcement | `-i sample.?limit` (no `_test.go`) | `scrape/target.go: return 0, errSampleLimit`; `scrape/scrape.go: case errors.Is(err, errSampleLimit)` |
| go-unknown | prometheus | default lookback | `-i lookback` (no `_test.go`) | `promql/engine.go: defaultLookbackDelta = 5 * time.Minute` |
| py-symbol | django | `get_object_or_404` | `^def get_object_or_404\(` / `get_object_or_404` | `django/shortcuts.py: def get_object_or_404(klass, *args, **kwargs):`; 14 usage files (including docs) |
| py-how | django | `QuerySet.get()` errors | `DoesNotExist\|MultipleObjectsReturned` | `django/db/models/query.py: raise self.model.MultipleObjectsReturned(`; `base.py: subclass_exception(` + `"DoesNotExist"` |
| py-unknown | django | too-many-fields guard | `-i too.?many.?fields\|max.?number.?fields` | `global_settings.py: DATA_UPLOAD_MAX_NUMBER_FIELDS = 1000`; `http/request.py` and `http/multipartparser.py: raise TooManyFieldsSent(` |
| java-symbol | guava | `firstNonNull` | `public static <T> T firstNonNull` / `firstNonNull` | `MoreObjects.java: public static <T> T firstNonNull(@Nullable T first, @Nullable T second) {`; 17 usage files |
| java-how | guava | size eviction | `maximumSize\|evictEntries` | `CacheBuilder.java: this.maximumSize = maximumSize;`; `LocalCache.java: while (totalWeight > maxSegmentWeight) {` |
| java-unknown | guava | default concurrency level | `-i concurrency.?level` | `CacheBuilder.java: DEFAULT_CONCURRENCY_LEVEL = 4` |

## Adding a PR, repo, or task

- **PR:** append to `PR_TASKS` in `tasks.mjs`. Required fields:
  - `owner`, `repo`, `number`, `question`
  - `candidate(path, file)`: a path-only filter an agent could apply from the inventory
  - `leanFileRe`/`leanLineRe`, or `leanSelect` (a jq boolean) for the gh-lean arm
  - `matchString` (octocode keyword) and `semantic` (the clasify `contribution` target)
  - `gt: { path: [markers] }`

  Optional fields: `metaOnly` (renames with no patch; the marker is the previous path) and `followup` (files without a patch: `filter`, `grep` literal, clasify `prefilter` and `locate` target).

  Derive the ground truth with `gh api --paginate --slurp …/files` plus `jq` before the first run. Never tune it to an arm's output.
- **Repo/task:** add the repo under `repos/` and append to `LOCAL_TASKS`:
  - `kind: symbol` needs `symbol` and `defPattern`
  - `kind: how` or `kind: unknown` needs `pattern`, optional `ci`/`exclude`, `questions[]` (clasify locate targets), and `gt`

  Then run `--freeze-gt` to record the usage lists and the repo commit.
- Keep the discovery inputs derivable from the question wording, and apply them to every arm.

## Baseline results

Run: `--label baseline`, 2026-09-29. The raw record is `results/bench-baseline.json`, which holds every step with its chars and ms.

### PR suite (all arms 4/4 correct)

| task | arm | chars | ≈tokens | calls | ms | read P | files read |
|---|---|--:|--:|--:|--:|--:|--:|
| ts-51387 (656 files) | gh-typical | 6,854k | 1,714k | 3 | 6.2k | 0.005 | 414 |
| | gh-lean | **3.8k** | 942 | 2 | 5.4k | 1 | 2 |
| | octocode | 86k | 22k | 11 | 14k | 1 | 2 |
| | octocode+clasify | 172k | 43k | 17 | 100k | 0.14 | 14 |
| rust-157558 (429 files, renames) | gh-typical | 1,417k | 354k | 3 | 5.7k | 0.007 | 425 |
| | gh-lean | **1.2k** | 311 | 2 | 2.9k | 1 | 3 |
| | octocode | 41k | 10k | 7 | 8.0k | 1 | 3 |
| | octocode+clasify | 43k | 11k | 8 | 21k | 1 | 3 |
| ts-61986 (huge patches) | gh-typical | 7,269k | 1,817k | 2 | 4.2k | 0.33 | 12 |
| | gh-lean | **40k** | 9.9k | 3 | 2.4k | 0.67 | 6 |
| | octocode | 101k | 25k | 9 | 12k | 0.67 | 6 |
| | octocode+clasify | 49k | 12k | 9 | 26k | 0.57 | 7 |
| tokio-8156 (37 files) | gh-typical | 49k | 12k | 2 | 2.8k | 0.08 | 37 |
| | gh-lean | **725** | 181 | 2 | 1.1k | 1 | 3 |
| | octocode | 19k | 4.8k | 3 | 2.9k | 1 | 3 |
| | octocode+clasify | 40k | 9.9k | 5 | 21k | 0.27 | 11 |

| arm | correct | chars | ≈tokens | calls | ms | files read |
|---|--:|--:|--:|--:|--:|--:|
| gh-typical | 4/4 | 15,590k | 3,898k | 10 | 19k | 888 |
| gh-lean | 4/4 | **45k** | **11k** | **9** | **12k** | 14 |
| octocode | 4/4 | 247k | 62k | 30 | 37k | 14 |
| octocode+clasify | 4/4 | 305k | 76k | 39 | 167k | 35 |

### Local suite

Cells show chars · calls · ms · correct (R = recall when wrong).

| task | rg/sed | octocode | octocode+clasify | octocode+clasify(files) |
|---|--:|--:|--:|--:|
| ts-symbol | 1.5k · 2c · 86ms · ✓ | 5.0k · 1c · 317ms · ✓ | 10.8k · 3c · 866ms · ✗ R0 | n/a |
| ts-how | 6.2k · 2c · 52ms · ✓ | 8.3k · 2c · 128ms · ✓ | 7.1k · 2c · 547ms · ✗ R0.5 | 7.6k · 3c · 720ms · ✓ |
| ts-unknown | 5.9k · 2c · 47ms · ✓ | 8.1k · 2c · 228ms · ✓ | 3.3k · 2c · 546ms · ✓ | 6.5k · 3c · 1440ms · ✓ |
| rust-symbol | 1.7k · 2c · 53ms · ✓ | 5.3k · 1c · 275ms · ✓ | 10.3k · 3c · 855ms · ✗ R0 | n/a |
| rust-how | 8.4k · 2c · 32ms · ✓ | 13.3k · 2c · 153ms · ✓ | 5.5k · 2c · 516ms · ✗ R0.5 | 13.9k · 3c · 815ms · ✓ |
| rust-unknown | 9.6k · 2c · 32ms · ✓ | 11.7k · 2c · 104ms · ✓ | 6.1k · 2c · 482ms · ✗ R0 | 6.9k · 3c · 662ms · ✓ |
| go-symbol | 208 · 2c · 106ms · ✓ | 1.0k · 1c · 204ms · ✓ | 6.6k · 3c · 789ms · ✓ | n/a |
| go-how | 10.3k · 2c · 29ms · ✓ | 11.4k · 2c · 159ms · ✗ R0.5 | 4.8k · 2c · 501ms · ✗ R0.5 | 11.6k · 3c · 871ms · ✓ |
| go-unknown | 11.4k · 2c · 40ms · ✓ | 10.6k · 2c · 159ms · ✓ | 5.4k · 2c · 486ms · ✗ R0 | 6.5k · 3c · 848ms · ✓ |
| py-symbol | 521 · 2c · 401ms · ✓ | 4.2k · 1c · 634ms · ✓ | 9.8k · 3c · 1061ms · ✓ | n/a |
| py-how | 11.5k · 2c · 25ms · ✓ | 17.2k · 2c · 325ms · ✓ | 9.3k · 2c · 580ms · ✗ R0.5 | 27.8k · 4c · 2410ms · ✓ |
| py-unknown | 9.4k · 2c · 127ms · ✓ | 11.7k · 2c · 212ms · ✓ | 8.5k · 2c · 597ms · ✗ R0.67 | 8.7k · 3c · 822ms · ✓ |
| java-symbol | 1.1k · 2c · 55ms · ✓ | 4.2k · 1c · 275ms · ✓ | 10.3k · 3c · 731ms · ✗ R0 | n/a |
| java-how | 14.2k · 2c · 26ms · ✗ R0.5 | 13.8k · 2c · 157ms · ✗ R0 | 6.8k · 2c · 497ms · ✗ R0 | 13.1k · 3c · 1100ms · ✓ |
| java-unknown | 12.7k · 2c · 25ms · ✓ | 13.7k · 2c · 158ms · ✓ | 3.9k · 2c · 448ms · ✗ R0 | 9.5k · 3c · 736ms · ✓ |

| arm | correct | chars | ≈tokens | calls | ms | mean recall | files read |
|---|--:|--:|--:|--:|--:|--:|--:|
| rg/sed | 14/15 | 105k | 26k | 30 | 1.1k | 0.97 | 40 |
| octocode | 13/15 | 139k | 35k | 25 | 3.5k | 0.90 | 40 |
| octocode+clasify (fileChunks) | 3/15 | 108k | 27k | 35 | 9.5k | 0.38 | 20 |
| octocode+clasify(files), how/unknown only | 10/10 | 112k | 28k | 31 | 10k | 1.00 | 17 |

On the same 10 how/unknown tasks:

| arm | correct | chars | ms | files read |
|---|--:|--:|--:|--:|
| rg/sed | 9 | 100k | 0.4k | 30 |
| octocode | 8 | 120k | 1.8k | 30 |
| clasify (fileChunks) | 1 | 61k | 5.2k | 15 |
| clasify (files) | **10** | 112k | 10.4k | **17** |

On the 5 symbol tasks, rg/sed used 4.9k chars and octocode used 19.8k. Both were 5/5 correct.

## Conclusions

**Headline.** Octocode is far better than *typical* gh, but on context size it loses to an *expert* gh or rg user. It never beats expert shell usage on chars in this suite. Clasify only pays off when it locates inside whole files, not when it screens search chunks or PR patches.

### Where octocode wins, and by how much

- **Against typical gh on PRs: 63× fewer chars.** 247k versus 15.6M, the same 4/4 correct. `gh pr diff` works on the 12-file TS PR, but the diff is 7.3M chars (dom.generated is 1.2 MB). On 656- and 429-file PRs the diff is refused, and the fallback files JSON is 1.4–6.9M chars. Octocode never needed a whole-diff dump. It also flagged files without a patch (`!tooLarge`) and reached the `dom.generated.d.ts` change with one `ghGetFileContent matchString` call (932 chars) instead of a 1.9 MB raw file.
- **Structure:** read precision is 1.0 on 3/4 PRs (only GT files opened) versus 0.005–0.08 for typical gh.
- **Fewer calls on local tasks:** 25 versus 30 for rg/sed, because one call batches several queries.
- **Clasify (files) is the most accurate local arm:** 10/10 versus rg 9/10 and octocode 8/10 on how/unknown tasks. It opens 43% fewer files (17 versus 30) at about 12% more chars than rg. It fixes both octocode misses (go-how and java-how) by ranking the deciding line anywhere in the file.

### Where octocode loses, and by how much

- **Against gh-lean on PRs: 5.4× more chars** (247k versus 45k) and 3.1× the wall time. gh-lean took 2–3 calls per PR; octocode took 3–11.
  - The inventory costs 30–38k chars on large PRs, which is 44–72% of octocode's total. `ts-51387` needs 7 inventory pages just to choose candidates.
  - `matchString` on selected patches still returns **whole patches** of matching files rather than the matching windows. That is 95k chars on ts-61986 (a single-line 70k baseline patch among them) and 45k on ts-51387 (the 33k Herebyfile patch).
  - Metadata costs about 3k per PR. Every inventory page repeats about 1.7k of it (0.4k PR header plus 1.3k `next.*`, about 27% of a 6.5k page), and patch responses repeat the full header including the body preview.
- **Against rg/sed on local tasks:** 1.33× chars (139k versus 105k), 3× wall time (3.5 s versus 1.1 s), and one fewer correct (13 versus 14).
  - Symbol tasks cost **4× chars** (19.8k versus 4.9k). In the `resultView:"files"` response, 58–71% of the text is a `next.clasify` hint (2.6k of 4.4k on tsx; 2.7k of 3.8k on guava).
  - Two misses (go-how, java-how) come from `maxMatchesPerFile` defaulting to 10. `scrape.go` has 25 hits and `CacheBuilder.java` has 31, and the deciding lines (2163 and 508) sit past the first 10. rg printed every hit, so it saw them in the search output.

### Did clasify help?

- **PR scouting (per-file patch resources): no.**
  - It costs 1.23× more chars and 4.5× more wall time than octocode without clasify: 305k versus 247k chars, 167 s versus 37 s. Scout calls take 14–78 s each.
  - It saved reads only on ts-61986, where it cut 101k to 49k by skipping large non-GT patches.
  - Discrimination is poor: it picked 14 of 45 candidates on ts-51387 (2 GT) and 11 of 37 on tokio-8156 (3 GT). Unrelated Miri test files scored 0.94–0.96, the same as the GT files.
  - Recall was perfect: every GT file scored ≥ 0.61 (most ≥ 0.9), so it never lost an answer.
  - A per-file `ghGetHistoryItem` resource captures 2–4 pages even for a 362-char patch, so the 25-cell budget forces matrices of at most 12 resources. On ts-61986 and ts-51387 the first attempt failed with `classificationExpandedCellsExceeded`: 8.3k chars and 9–10 s lost per failure.
- **Local clasify over `localSearch candidateEvidence:"fileChunks"`: harmful.**
  - Only 3/15 correct, against 13–14/15 for the non-clasify arms.
  - `fileChunks` judges one bounded chunk of about 120 lines per file (at most 5 files), so the deciding line is often outside the chunk. Examples: `history.ts:11-34` versus the answer at 180, and `builder.rs:622-645` versus 324.
  - For symbol definitions, the defining file is often not among the 5 chunked files (tsx, tokio, guava: 0/3).
  - About 55% of each fileChunks result is boilerplate: a repeated `limitations` string, per-page `next.read`, and `modified`.
- **Local clasify locate over whole top files (the `(files)` variant): yes, for accuracy.**
  - It is the only arm at 10/10, and it reads the fewest files.
  - The cost is latency (about 1 s per task versus 0.04 s for rg) and one extra call.
  - With 2 questions over 3 files it can exceed 25 cells (py-how), which costs a failed call of about 12k chars before the per-question split.

### Optimization targets, ranked by impact

1. **`ghGetHistoryItem` `matchString` should return matching hunks (±context), not whole patches.** Evidence: 95k of 101k on ts-61986 and 45k of 86k on ts-51387 were whole-patch text. gh-lean shows the same answers in 40k and 3.8k. This closes most of the 5.4× gap with gh-lean.
2. **Cheaper PR inventory:** a path filter, glob, or `matchString` on `content.changedFiles`, plus no header or `next.*` on page 2+. Evidence: the inventory is 38k (44%) of ts-51387 and 29k (72%) of rust-157558. Each inventory page repeats about 1.7k of header and hints (about 27% of a page), and each patch response repeats the full PR header and body preview.
3. **`localSearch` hit truncation:** when a file has more hits than `maxMatchesPerFile`, rank decisive hits (declaration, assignment, `raise`/`return err`) before comments, or raise the default for small result sets. Evidence: both octocode local misses (go-how, java-how) are hits 11+ in a file; rg saw them at the same char budget.
4. **clasify `fileChunks`:** cover every hit cluster (or the prefiltered 600-line windows used by `localFetch prefilter`) instead of one ~120-line chunk per file, and include `path` in `best`. Evidence: fileChunks scored 3/15 versus 10/10 for whole-file locate on the same tasks. `best` entries only carry `resourceId`, which is ambiguous when one search resource spans 5 files.
5. **clasify output and budget ergonomics.**
   - Drop the per-page `limitations`, `modified` and `next.read` repetition (about 55% of a fileChunks result).
   - Trim the `next.clasify` hint on `resultView:"files"` (58–71% of the output, making the symbol tasks 4× rg).
   - Collapse `classificationExpandedCellsExceeded` into one error per matrix instead of one per page (8–12k chars per failed call).
   - Make per-file `ghGetHistoryItem` patch resources capture only the patch (not 2–4 pages), so one matrix can screen 25 files.
   - Latency: PR scout calls of 14–78 s make clasify 4.5× slower than plain octocode on PR review.

Also worth noting: PR-scout precision (P ≥ 0.5 selects 30–40% of candidates) is too low to save reads when a literal keyword exists. Prefer `matchString` (once target 1 lands) and reserve clasify for semantic questions without a literal.
