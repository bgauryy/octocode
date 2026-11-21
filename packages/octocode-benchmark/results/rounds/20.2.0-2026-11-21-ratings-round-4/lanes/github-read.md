done. I rated all four GitHub read tools on the pinned build `octocode 20.2.0+56f67471` (SHA-256 `e246ff42…a405f`, HEAD `e9309aa35`, dirty tree). Every row, set and page walk I checked matched GitHub exactly (`gh api` GET and raw file reads). The one wrong answer was a PHP block read, and the output disclosed it as an estimate. Full report: `.octocode/code/temp/agents/olive-newt/REPORT.md`.

| tool | Desc | Schema | Workflow | Clarity | Time | Tokens | Paging | Smart | Short | Long | Stress | Hints | Overall | Correct |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| ghSearchRepo | 9 | 8 | 9 | 9 | 8 | 9 | 10 | 9 | 9 | 9 | 9 | 7 | **9** | 32/32 |
| ghSearchCode | 9 | 8 | 8 | 8 | 8 | 8 | 10 | 7 | 9 | 9 | 9 | 7 | **8.5** | 28/28 |
| ghStructure | 9 | 8 | 9 | 9 | 9 | 9 | 10 | 9 | 9 | 9 | 9 | 9 | **9** | 25/25 |
| ghGetFileContent | 9 | 9 | 9 | 8 | 9 | 8 | 9 | 8 | 9 | 9 | 9 | 9 | **8.5** | 24/25 |

Calls / output bytes: repo 17 / 24.7K, code 14 / 40.3K, structure 9 / 33.9K, file 32 / 457K. The file total includes two whole-file walks.

**How I checked:** I walked every page of five answers:
- Zig repos: 157 = gh 157
- tokio `spawn_blocking` code search: 67 = 67
- tokio recursive tree: 892 files = 892
- `defs.rs`: 7,675 lines rebuilt with 0 mismatches
- a 140 KB minified JS file: rebuilt from 10 pages

Tag SHAs come back as the commits the tags point to. Line numbers in code-search hits matched the raw files.

**Defects:**
1. **med, native** – ghGetFileContent `block:true` on a PHP method whose `{` sits on its own line returns only the signature. laravel `Str.php` `slug(` gives 1665–1665 (real block 1665–1688). symfony `Application.php` `run(` gives 196–196 (real 196–285). Those rows also lack `matchedLines`.
2. **low-med, native + core** – Keywords that look like search qualifiers are searched as plain text and come back empty. Examples: `language:rust` in ghSearchCode and ghSearchRepo, `"ripgrep stars:>1000"` in ghSearchRepo. The hint only says to drop a term; it never points to `extensions`, `filters.language` or `filters.stars`. Same family as R3-H5.
3. **low, native** – A paged minified-file read changes unit on its last page: pages 1–9 are `bytes`, page 10 is `lines` with a `7\t` prefix. Page 1 also reports `returnedLines:6` while serving 17 KB of 140 KB.
4. **low, native** – A `ref` code search (R3-GR1) now warns correctly but gives no runnable call to search at that ref.
5. **low, core** – ghSearchRepo's output reference still lacks `defaultBranch`. Responses also carry `expiresAt` on a last page that has no next call.
6. **low, native** – ghStructure `operation:"tags"` gets a plain refusal instead of being repaired to `refs`.

**Round-3 issues re-tested:**
- **Fixed:** GR3 (tag spelled with or without `v`: the error names the existing tag and gives a call that reads it), GR4 (owner given as a URL or `owner/repo` is repaired), GR5 (empty refs glob is marked empty with a lead).
- **Partly fixed:** GR1 (defect 4), GR2 (Ruby and Kotlin blocks are now right; PHP is wrong, defect 1), GR6 (`returnedLines` and ghStructure `defaultBranch` are now documented; the unit switch remains, defects 3 and 5).
- **R3-F1:** the instructions now say `refs`.

**Good:** No row was lost or duplicated in any walk. Error rows name the near-miss ref or path and give a related call that runs unchanged.

**Unverified:** One cold 5-query ghSearchRepo batch took 16.8 s. I could not reproduce it: each query alone ran in 0.04–0.97 s.

Noticed: GitHub code-search quota is shared, and I hit HTTP 403 once around 22:06 UTC (other lanes may see it too). `square/okhttp` is now `lysine-dev/okhttp` (confirmed by gh). `.env.example` files are withheld as credential files. My materialize call left a cache under `~/.octocode/tmp/materialize/v5/sharkdp/fd/`.

For coordinator: 10 `octocode serve` processes are running, started between 00:00 and 00:09; this lane's calls started some of them. The "before" PID file is empty, so stop them by PID during cleanup. Defect 1 is the one to spot-check (two raw files cited). I deleted the lane's raw folder `.octocode/tmp/rate4-ghread`; only the report in my agent scratch folder remains. B23 is still ongoing and waits for your merge.

Ask user: none