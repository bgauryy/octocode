# Comparisons

For each Octocode tool and workflow, this page names the competitor an expert would use for the
same question and says what "better" means. It is the one owner of the expert commands: the
[tool checks](CHECKS.md) and the
[check briefs](briefs/) link here instead of repeating them. [METHOD.md](METHOD.md) says how to
run and judge a comparison.

A comparison is meaningful only when both sides answer the same question. Where the competitor
answers a narrower question (`rg -w` finds spellings, not the symbol a reference denotes),
measure what it gets wrong. Where nothing answers the question, the row says so and records
Octocode's answer alone.

## The rule for every row

- **Smart, not plain.** The competitor is the command an expert would actually type: projected
  fields, a region rather than a whole file, a count before content. A plain dump that an expert
  would never run proves nothing.
- **Correctness gates everything.** Characters and time count only when both answers are right.
  A smaller wrong answer loses.
- **Record per row:** right on both? (with the deciding source line), characters (≈ 4 per
  token), calls, Octocode first and repeat time, competitor time, and what the expert had to
  know in advance: flags, REST paths, field names, jq, a guessed function length.
- **Octocode loses a row** when the competitor is right, smaller and needed no guessing. Say so
  plainly; that row is the improvement target.

For CLI orchestration and continuation checks, capture `--json`, or retain both text streams.
The default text renderer puts counters and optional `related[]` commands on stderr; source
facts and required continuation commands remain on stdout. A stdout-only capture misses those
notes. Count stdout plus stderr when comparing total text output, and record which representation
each character count measures.

Time the interface an agent actually uses. A fresh Node CLI client, a direct native client and
a persistent MCP request have different startup costs even when they share a warm runtime.
The advanced round (in git history, see [results/README.md](results/README.md#rounds-tracked)) measures them
separately. Compare natural text and JSON before attributing extra characters to the engine;
strip only shared benchmark environment bookkeeping from logical inputs, retaining setup once.

## Map

| Octocode | Competitor | Same question? | Stored evidence | Status |
|---|---|---|---|---|
| [localSearch](#localsearch) | `rg` | yes | rg comparisons, 2026-10 | measured |
| [localFetch](#localfetch) | `sed -n`, ast-grep range + `sed` | yes; derive the whole declaration's range | advanced round, 2026-11-14 | measured |
| [structureSearch](#structuresearch) | `fd`, `find`, `stat`, `wc -l` | yes | advanced round, 2026-11-14 | measured |
| [astSearch](#astsearch) | `ast-grep` | yes | ast-grep comparisons, 2026-10 | measured |
| [lspSearch](#lspsearch) | same language server over direct LSP; `rg -w` | direct LSP yes; rg only spelling | advanced round, 2026-11-14 | selected semantic sets measured |
| [graphBuild and graphQuery](#graphbuild-and-graphquery) | Graphify; lspSearch, rg, ast-grep; madge, dependency-cruiser (TS) | callers, imports, cycles, impact, paths | graph-vs-tools round, 2026-11-23; advanced round, 2026-11-14 | measured; Rust callers recall is the gap |
| [clasify](#clasify) | the main agent reading everything | yes | clasify A/B campaigns | measured; adoption is the gap |
| [ghSearchRepo](#ghsearchrepo) | `gh search repos`, `gh repo view` | yes | gh-lean campaigns | measured |
| [ghSearchCode](#ghsearchcode) | `gh search code` | yes | gh-lean campaigns | measured |
| [ghStructure](#ghstructure) | `gh api …/git/trees`, `gh repo read-dir`, tags | yes | gh-lean campaigns | measured |
| [ghGetFileContent](#ghgetfilecontent) | raw contents + `sed -n`, `gh repo read-file` | yes | gh-lean campaigns | measured |
| [ghSearchHistory and ghGetHistoryItem](#ghsearchhistory-and-ghgethistoryitem) | `gh search prs`, `gh api commits`, `gh pr view`, compare | yes | gh-lean campaigns | measured |
| [ghCloneRepo](#ghclonerepo) | `gh repo clone -- --depth 1` | yes | none | check rounds only |
| [artifactSearch](#artifactsearch) | `npm view`, registry JSON | yes | package lookup, 2026-11-04 | characters only |
| [securityCheck](#securitycheck) | OSV API, GitHub advisories | yes | none | not yet measured |
| [Research workflows](#research-workflows) | gh, gh-lean, gh + RTK, gh + Headroom; lean shell | yes | GitHub 30 and local 30 campaigns | measured |
| [Output compressors](#output-compressors) | RTK, Headroom | yes | August 2026 campaigns | measured (numbers only) |
| [Version against version](#version-against-version) | the previous Octocode release | yes | v18 vs v20 timing | measured |

The 2026-11-14 advanced round (in git history, see [results/README.md](results/README.md#rounds-tracked))
rechecked rg 15.2.0, ast-grep 0.45.0 (`sg` is the same binary), gh 2.96.0, and the installed
TypeScript language server 6.0.2 with TypeScript 6.0.3. It installed `graphifyy` 0.9.108 into an
isolated temporary environment, without global agent hooks or a model backend. Earlier checks
(2026-11-13) recorded rtk 0.44.2, headroom 0.34.0, fd 10.5.0 and jq 1.7.1; they are not all fresh
version assertions. Check the exact executables and versions each round.

## Local tools

### localSearch

- **Question:** where a term is used, which files hold or lack it, where it is used most, and a
  term inside a minified one-line file.
- **Expert form:** `rg --no-config -n -w -F TERM DIR -g '*.rs'` (name order: add `--sort path`); matching-line counts
  `rg --no-config -c -F TERM DIR`; files `rg -l` and `rg --files-without-match`;
  a window in a long line `rg -o -b '.{0,60}TERM.{0,60}' FILE`.
- **First page:** the default walk order stops when the page fills, so compare page 1 against `rg --sort path TERM DIR | head`, not against a whole-scope count; a content page that has more states no total, and exact totals come from a count view.
- **Better means:** the same match set first. Then characters, a window with its column for a hit
  in a long line, a continuation that reaches every remaining hit, and the next call ready from
  the row. rg is expected to win raw process latency; say so when it does.
- **Result (2026-10-20, raw evidence no longer stored):** both agree on all 21 lexical cases; rg is faster.

Use `--no-config` for reproducible comparisons. Match ignore, hidden-file, binary, case and
word-boundary policies before timing; use `-F` for literal questions and `-P` or `-U` only when
their semantics are needed. `rg -c` counts matching lines; `rg --count-matches` counts individual
matches. Omit `-m` and `head` when checking the complete evidence set. See the
[ripgrep guide](https://github.com/BurntSushi/ripgrep/blob/master/GUIDE.md).

### localFetch

- **Question:** lines A–B of a file; one function's body, short and over 400 lines; the outline
  of a large file.
- **Expert form:** `sed -n 'A,Bp' FILE` for a known range; an ast-grep declaration match with
  JSON range fields followed by `sed` for a whole function. Derive the range before the read;
  a guessed `rg -A40` window is not a complete-declaration baseline.
- **Better means:** exact requested source, including declarations over 400 lines, with all
  required pages read. Count AST discovery and the dependent range read. State whether attached
  comments or line numbers make the representations differ. Use a full-file read only when the
  question requests the full file; byte prefixes are not evidence-complete alternatives.

### structureSearch

- **Question:** the layout of a folder; the largest, longest and newest files under it; files by
  name.
- **Expert form:** `fd -t f PATTERN DIR`; `find DIR -maxdepth 2 -type d`;
  `find DIR -name '*.rs' -exec stat -f '%z %N' {} + | sort -rn`; `wc -l`.
- **Better means:** the same listing; one metric per row for a sorted listing; characters.

### astSearch

- **Question:** calls of a function, not comments, strings or definitions; calls inside one
  function; declarations of a file or folder.
- **Expert form:** `ast-grep run -l rust -p 'NAME($$$A)' DIR`,
  `ast-grep run -l rust -p '$X.NAME($$$A)' DIR`, and
  `ast-grep scan --inline-rules '…'` for `has` and `inside` rules. The plain text form `rg -n 'NAME\('`
  also returns definitions, comments and strings: count those as its errors.
- **Better means:** the same match set (agreement is the correctness gate), only the shape asked
  for, then characters and time. An empty bare pattern should say how many receiver calls exist
  and offer the rule; ast-grep says nothing.
- **Result (2026-10-20, raw evidence no longer stored):** agreement on all 41 shared and 17 edge cases.

### lspSearch

- **Question:** who references a symbol, its definition, callers and callees.
- **Competitor:** a direct client of the same language server for definitions, references and
  incoming calls. `rg -n -w NAME` is a separate spelling baseline: it cannot establish symbol
  identity and can miss renamed references while including shadows, comments and strings.
- **Better means:** equal source-confirmed locations and caller sites first. Record server
  version, initialization, document synchronization, readiness, first request and warm requests
  separately. Preserve reacquisition after an idle server is retired; it is not a warm sample.
  Check source snippets, roles and diagnostics as additional evidence, not duplicate locations.
- **Accounting:** `didOpen` source bytes are client/server traffic, not model context unless
  the model actually reads them. Compare raw LSP results, Octocode output and a shared semantic
  projection separately. A partial-analysis warning remains relevant even when the selected
  oracle set matches. The advanced round observed TypeScript diagnostic 7006 and disclosed it.

### graphBuild and graphQuery

The 2026-11-23 round ([report](results/rounds/20.2.0-2026-11-23-graph-vs-tools/report.md), [data](results/rounds/20.2.0-2026-11-23-graph-vs-tools/data/)) measures the saved Octocode
Graph (`octocode-graph query`) against native graphQuery, lspSearch, rg, ast-grep, madge,
dependency-cruiser and Graphify (0.9.108, code-only). It uses two targets: `packages/octocode-native` (Rust,
3,516 files, 19 crates) and a frozen copy of `packages/octocode-graph/src` (TypeScript, 70 files). Ground
truth is every call site of 33 uniquely named functions, read by hand, and the import statements resolved by
the TypeScript scanner.

| Callers (site precision / recall) | Rust, 143 sites | TypeScript, 63 sites | Warm s per question (Rust) |
|---|---|---|---:|
| Octocode Graph `callers --tests` | 1.00 / **0.57** | 1.00 / 1.00 | 0.36 |
| native graphQuery (one crate) | 1.00 / 0.27 | 1.00 / 0.76 | 0.44 |
| lspSearch `callers` | 1.00 / 0.97 | 1.00 / 1.00 | 0.09 |
| `rg` call pattern (unique names only) | 1.00 / 1.00 | 1.00 / 1.00 | 0.06 |
| ast-grep, 3 patterns | 1.00 / 0.85 | 1.00 / 1.00 | 0.73 |
| Graphify `explain --all` | 1.00 / 0.24 | 1.00 / 0.71 | 2.14 |

- **Rust callers are where Graph is worse.** It missed 44 of 72 cross-crate sites: the ones it found all came
  from LSP evidence, and name binding did not follow `pub use` re-exports. It also missed 13 of 22 calls
  inside macros. Its answer still reported `isPartial: false`.
- **Depth is where Graph is ahead.** Only Graph answers cross-crate transitive impact in one call (697 files
  across 11 crates for `result.rs`, 0.36 s).
- **On TypeScript, Graph was exact on every check:**
  - transitive impact for 3 files;
  - 167 of 167 import pairs, the same as madge, dependency-cruiser with type-only imports, and an `rg` walk;
  - both injected cycles.

  dependency-cruiser's default drops type-only imports (114 pairs) and so missed the type-only cycle. A
  madge `--depends` walk needs 14–24 calls and 17–32 s per file.
- **Build:** 127 s on Rust (76 s of it LSP), against Graphify's 21–23 s code-only. On TypeScript, 1.5–5.2 s
  with LSP and 1.4 s with `--lsp off`.

That round restricts the sample to unique names, which flatters rg and ast-grep. It does not supersede the
Graphify claims below, which were measured on other targets. The one addition: on the Rust workspace,
Graphify found 34 of 143 sites (6 of 72 cross-crate) and kept one location per caller edge.

The advanced round (in git history, see [results/README.md](results/README.md#rounds-tracked)) runs the official
[Graphify](https://github.com/Graphify-Labs/graphify) package (`graphifyy` 0.9.108), code-only,
without clustering, on a frozen JavaScript fixture and pinned `pallets/itsdangerous` source.
The earlier 2026-10-28 source-only comparison does not describe this measured round.

- **Questions both can answer:** the most imported files (dependents), callers of a function,
  the shortest path between two symbols, what may be affected by a change (graphify's
  "affected"), the most connected nodes (graphify's "god nodes" against `hubs`).
- **Shared evidence:** both tools resolved the tested aliases and re-exports and recovered the
  same selected caller sets. Both provide source evidence. In the real-source case, Octocode
  exposed all 19 call sites across 16 callers; Graphify's examined graph and `explain --all`
  retained one location per caller-to-target edge. Distinguish sites from caller nodes.
- **Different models:** Graphify also represents configuration keys, containment and other
  relations. Raw hub degrees therefore answer different questions; project the same module
  import edges before comparing fan-in. Its broader advertised multimodal and model-assisted
  features were not exercised in this code-only round.
- **Freshness and cycles:** after a source edit, Octocode identified the stale snapshot and
  changed file; Graphify's tested path command returned the old path without a stale warning.
  Both refreshed to the correct new path. Octocode directly reported the tested import cycle;
  projecting a cycle from Graphify's exported edges is a custom step, not a native CLI command.
  A graph's runtime import classification is not proof that code executed.
- **Shell fallback:** importers with `rg -o "from '[^']*'" DIR | sort | uniq -c | sort -rn`;
  co-change from `git log --format= --name-only -- PATH`. Lexical import counts are candidates;
  they do not establish resolved dependency cycles or unused exports.
- **Better means:** edge precision and recall on a sample confirmed against the source (with
  lspSearch or by reading), build time and size on the same repository, query characters, and
  claims made without a witness. Use the commands of the installed graphify version
  (`graphify --help`); don't assume them.

### clasify

- **Question:** which of 300 log lines, 60 files or 200 search hits are about X.
- **Competitor:** the main agent reading every candidate itself (search output read in full,
  or files read one by one).
- **Better means:** the same answer with fewer main-agent characters, the billed request count,
  and recall: the same query run exactly, without `clasify`, shows what the answers left out. Savings figures are owned by
  the [benchmark README](README.md#main-agent-tokens-with-and-without-jev).
- **Stored evidence:** `results/mcp-clasify-ab-111154-2026-10-09/`, `results/gated-jev-*`,
  `results/selective-guidance-*`. When clasify was offered, agents did not call it, so no saving
  appeared; adoption is the open question, not the tool's answer.

## GitHub and package tools

The lean `gh` style is in [briefs/arms/gh-lean.md](briefs/arms/gh-lean.md). Live targets are
public repositories, more than one before a result counts as a defect, never `facebook/react`.

Project only the fields the question needs. For a complete collection, use `gh api --paginate`
(GraphQL also needs `pageInfo` and `$endCursor`); projection alone does not fetch later pages.
Record `--cache` usage and separate first-process, repeated and origin-revalidated requests:
a warm process is not proof of a cache hit. Do not clear the user's cache for a benchmark.
See the [GitHub CLI API manual](https://cli.github.com/manual/gh_api).

### ghSearchRepo

- **Expert form:** `gh search repos TERM --sort stars --json fullName,stargazersCount --jq …`;
  `gh repo view O/R --json nameWithOwner,description,stargazerCount`.
- **Better means:** the right rows, identity for the next call, characters.

### ghSearchCode

- **Expert form:** `gh search code TERM --repo O/R --json path,textMatches --jq …`.
- **Better means:** a hit with line numbers and the commit for the next read; a hit that
  answers ends the question.

### ghStructure

- **Expert form:** `gh api 'repos/O/R/git/trees/SHA:sub/dir' --jq '.tree[].path'`, with
  `?recursive=1` only on a narrow subtree; `gh repo read-dir DIR --repo O/R --json path,type,size`;
  tags `gh api repos/O/R/tags --jq '.[].name'`.
- **Better means:** the same listing, sizes when asked, truncation disclosed (REST recursive
  trees truncate silently unless `truncated` is checked), a tag resolved to its commit.

### ghGetFileContent

- **Expert form:** `gh api 'repos/O/R/contents/PATH?ref=SHA' -H 'Accept: application/vnd.github.raw' | sed -n 'A,Bp'`;
  `gh repo read-file PATH --repo O/R --ref SHA | sed -n 'A,Bp'`.
- **Better means:** a region, block or outline without fetching the whole file into context;
  for a word in a minified file, a window instead of the line. `gh` has no server-side region
  read, so the whole file is fetched either way; what counts is what reaches the model.

### ghSearchHistory and ghGetHistoryItem

- **Expert form:** `gh search prs TERM --repo O/R --merged --json number,title --jq …`;
  `gh api 'repos/O/R/commits?path=P&per_page=3&page=N' --jq …`;
  `gh pr view N --repo O/R --json files,reviews --jq …`;
  `gh api repos/O/R/compare/A...B --jq '.files[] | select(.filename=="P") | .patch'`;
  `gh issue view N --repo O/R --json comments --jq …`.
- **Better means:** the patch for one path, not the whole compare; paging by cursor against
  rebuilding `page=N`; the commit, number and path for the next call.

### ghCloneRepo

- **Expert form:** `gh repo clone O/R DIR -- --depth 1`, then `rg`.
- **Better means:** time to the first local answer, then the local-tool rows above.

### artifactSearch

- **Expert form:** `npm view PKG@V version repository gitHead --json`;
  `curl -sL https://pypi.org/pypi/PKG/json | jq '.info.version'`;
  `curl -sL -A <client> https://crates.io/api/v1/crates/PKG | jq '.crate.max_stable_version'`.
- **Better means:** the same version, repository, directory and commit, and a file read at that
  commit in the next call; characters.
- **Stored evidence (2026-11-04, characters only, one run):**

  | Package | artifactSearch | Registry's full JSON | `npm view` with named fields |
  |---|---:|---:|---:|
  | `zod` (npm) | 731 | 89,723 | 147 |
  | `typescript` (npm) | 1,637 | 318,171 | 1,198 |
  | `react` (npm) | 686 | 337,209 | 145 |
  | `requests` (PyPI) | 695 | 192,961 | not applicable |
  | `serde` (crates.io) | 732 | 440,993 | not applicable |

  `npm view` with named fields is smaller; artifactSearch also ties the version to its source
  commit. Answer quality was not judged and time was not compared (development build).

### securityCheck

- **Expert form:** `curl -s -d '{"package":{"name":"PKG","ecosystem":"npm"},"version":"V"}' https://api.osv.dev/v1/query | jq '.vulns[].id'`;
  advisories for a package with `gh api 'advisories?affects=PKG' --jq …`, one advisory with
  `gh api advisories/GHSA-… --jq …`. osv-scanner answers the same question
  from a lockfile and is not installed. `npm audit` answers a different question (the installed
  tree) and is not a competitor.
- **Better means:** the same affected verdict and the same version to upgrade to; characters;
  a disclosed rate limit instead of a silent wait when no token is set.

## Workflows

### Research workflows

- **Question:** a whole research question answered end to end, from the
  [question sets](questions/README.md).
- **Competitors:** for GitHub, plain gh, gh-lean, gh + RTK and gh + Headroom; for local code,
  the lean shell (`rg`, `ast-grep`, `find`, `sed -n`, read-only `git`). The arm briefs are in
  [briefs/arms/](briefs/arms/).
- **Better means:** blind-judged correctness first, then characters, calls, failed calls and
  time per question, aggregated per [METHOD.md](METHOD.md#aggregate).
- **Stored evidence:** the [results index](results/README.md) and the headline in the
  [README](README.md#results).

### Output compressors

- **Question:** does compressing a command's output reach Octocode's size without losing the
  evidence the answer needs?
- **Competitor:** the same `gh`, `rg` or `git` command with and without `rtk` or Headroom.
- **Better means:** the compression ratio against evidence kept: does the answer built from the
  compressed output still hold? In August 2026 Headroom lowered correctness to 8.66, the lowest
  of the setups. Passing Octocode's own output through them is an open, cheap question.

### Version against version

- **Question:** is the new build at least as correct, leaner and faster than the last release?
- **Competitor:** the previous release, through `npx -y octocode@<version>`.
- **Better means:** correctness, characters, and time warm (session server running) and cold
  (nothing running), on the same machine, interleaved. Time release builds only.
