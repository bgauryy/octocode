# GitHub response quality and efficiency: Octocode versus gh

## Executive summary

Octocode's strongest opportunity is a lower cost per verified answer: acquire only the requested evidence, preserve its revision, and reduce the agent calls needed to finish the question. A remote speed victory over optimized `gh` has **not** been demonstrated in this environment.

Concrete results:

- Local `--version` startup medians were **10 ms native Octocode**, **60 ms gh**, and **80 ms Octocode's Node launcher**, across five interleaved samples per arm. This measures startup and exit only, under changing host load.
- The existing Vite search returned the same first two paths in **743 characters** with `fields:["path"]`, versus **1363 characters** with verified line content. That is a smaller answer for a paths-only question, not equal selected evidence or a causal latency experiment.
- A complete five-line block returned **718 characters of raw text-body JSON**. Its numbered content string was **187 characters** (167 without line prefixes), and its serialized optional `related` array was **262 characters**. Removing that navigation property would produce an estimated 445-character answer. It was retained pending workflow evidence.
- The benchmark guard incorrectly rejected installed `gh repo read-file` and `gh repo read-dir`. The baseline and guard are corrected; six focused guard tests pass.
- Native source changes now preserve known indexed commits in path-search handoffs, combine eligible PR headers and single connections in one GraphQL request, omit unselected labels, and use REST when GraphQL cannot preserve file metadata. Native execution remains pending.

[Raw measurements and exact probe inputs](2026-11-13-github-vs-gh-results.json) accompany this report. These are observations and source-derived expectations, not a release acceptance certificate.

## Method and limitations

Compared installed **gh 2.96.0**, **Octocode 20.2.0**, and Node **26.4.0** on macOS arm64. Both native executables identify as arm64. The source tree contains extensive concurrent work; this turn's edits are identified below.

The shell cannot successfully query GitHub: `gh api` reported a connection failure, and its account validation failed in this restricted context. Token validity outside this context is unknown. The connected Octocode MCP can read public GitHub repositories, but its RPC path and server state differ from shell execution. Therefore its elapsed times are **not** paired `gh` latency measurements.

Initial host load averages were **91.50 / 80.98 / 66.18**. Later load fell below 12 logical CPUs. Memory pressure and process inspection were denied; later Node host statistics reported 32 GiB total and 5.23 GiB free. No production inference calls were made. No cache clearing, credential changes or competing-session termination was performed.

Local startup calls ran sequentially in five rounds, rotating the three arms. An earlier overlapping pilot is excluded. The native binary was pinned by hard link during measurement; no reusable benchmark harness was saved. zsh `TIMEFMT` provided elapsed/user/system time and reported maximum RSS. Its displayed time resolution was 10 ms. RSS is not synchronized whole-process-tree peak RAM, and these runs do not measure a long-lived MCP server or actual GitHub tool memory.

Binary identities:

| Artifact | SHA-256 | Size |
|---|---|---:|
| Native Octocode | `80a66639a63a266bce97c6ff9592d2fe7f93bb3ea7bb3e9159d2876160c6058e` | 264534176 bytes |
| gh | `02d2d4a85241c6a8c0b77ebb1ec76fc723caf7fb128e00915b306b968847cba1` | 38299474 bytes |
| Octocode Node launcher | `62094ac410d37ea37714b330393e9b8de413b37deaf69a45c219c2f41d8717e2` | 23772 bytes plus its dependencies |

The native build profile is unverified. Its larger on-disk artifact is real, but this is not an optimized-release size comparison. The connected MCP server's binary hash is unknown. The shared staged native binary was replaced before the final inspection; the direct-native arm retained its pin, while the Node launcher's exact child-artifact binding is unverified. Its timings describe the observed launcher path, not an isolated measurement of wrapper overhead. Measured binaries do not establish runtime behavior of the new source changes.

## Measurements

### Local startup

| Command class | Median elapsed | Range | Median reported maximum RSS | Median user + system CPU |
|---|---:|---:|---:|---:|
| Native `octocode --version` | 10 ms | 10–20 ms | 12.5 MiB | Below displayed 10 ms resolution |
| `gh --version` | 60 ms | 50–580 ms | 37.6 MiB | 60 ms |
| `node .../octocode.js --version` | 80 ms | 60–700 ms | 47.8 MiB | 70 ms |

**Implication:** the native entry point and persistent MCP path avoid repeated Node startup work. The observed Node launcher median exceeded gh in this small sample, with the artifact-binding caveat above. None of these numbers establishes remote API speed, throughput, cache-heavy behavior or full-tool memory efficiency.

### Existing connected-MCP responses

Counts are raw `content[0].text` characters and UTF-8 bytes; wrapped MCP JSON is separately recorded in the data file. Elapsed time includes RPC, admission, provider work and runtime shaping; their contributions are unknown. First observed calls are not established cold-cache calls.

| Probe | Elapsed | Characters / bytes | Completeness |
|---|---:|---:|---|
| Vite verified content search, first page | 36534 ms | 1363 / 1363 | First 2 of 7 files |
| Vite paths-only search, first page | 1204 ms | 743 / 743 | Same first 2 paths; later pages unread |
| Vite complete five-line block | 1420 ms | 718 / 718 | Lines 506–510 |
| Same pinned block repeated | 263 ms | 718 / 718 | Identical selection |
| Cargo paths-only search | 8721 ms | 676 / 676 | First 2 of 25; later pages unread |
| Cargo pinned range | 3606 ms | 777 / 777 | Selected lines 1–12 complete |
| Cargo exact-file `Workspace` search | 1855 ms | 1743 / 1745 | One file; 13 verified global-line matches |
| Cargo emitted function read | 19451 ms | 2604 / 2604 | Function 178–226 plus doc line 177 |

The deliberately tiny Vite content walk terminated after four calls at offsets 0, 2, 4 and 6, reconstructing seven unique files at a stable commit. It consumed **72127 ms** and **3800 raw text characters**. This is a pagination-integrity probe, not a representative default-latency benchmark. The projection and repeat observations are confounded by execution order, cache state, rate admission and host load; their latency ratios are not causal speedups.

## Compare with optimized gh

The competitor baseline must include its actual capabilities:

| Task / property | Strong gh baseline | Octocode implication |
|---|---|---|
| Known repository facts | `gh repo view --json ...` pushes requested fields into its GraphQL repository read and skips README acquisition. | Aim for request-count parity and concise output. Arbitrary REST metadata dumping is a weak comparator. |
| Path/snippet discovery | `gh search code --json path,textMatches` returns index evidence. | Compare paths-only discovery with paths-only discovery. Verified global lines require source acquisition and have a different contract. |
| File region at a known commit | Preview `gh repo read-file PATH --ref SHA` streams raw text to `sed` or `grep`; raw Contents media is another valid form. | Octocode's useful distinction is direct block/outline selection and stable source coordinates, especially when declaration bounds are unknown. |
| One directory | Preview `gh repo read-dir` uses GraphQL; projected entries remain wrapped in `{gitSHA,id,entries}`. | A whole recursive REST tree dump is not an expert baseline for a known directory. |
| Source identity | read-file `gitSHA` is a blob; read-dir `gitSHA` is a tree. | Pin the same explicit containing commit in both arms; never equate a blob/tree SHA with a revision. |
| PR sections | `gh pr view --json files,reviews,headRefOid,...` selects fields at the provider. | The new one-request PR path closes an avoidable request gap; it is not unique to Octocode. Missing rename evidence still requires REST. |
| All selected pages | `gh api --paginate` walks automatically; GraphQL needs the cursor variable and `pageInfo`. | Execute Octocode continuations fully. Do not claim gh lacks pagination, or compare a complete result with a prefix. |
| Small REST stdout | `gh api --jq` filters after the response has arrived. | Measure provider bytes separately from model-visible bytes. Small output alone does not prove less network work. |
| Explicit TTL reuse | `gh api --cache 5m` can return a TTL-valid cached response without a provider roundtrip. Its key includes Authorization. | Separate permitted-staleness tests from strict-current/revalidated reads. Do not weaken freshness or credential partitioning for a speed score. |
| Tags | GitHub's returned tag order is different from highest semantic version. | Match ordering semantics before comparing requests or latency. |

The preview helpers are installed and documented, but are subject to upstream change. read-file JSON `content` is base64: use its streamed raw mode for region reads. read-dir accepts **entry fields** such as `path,type,size`, and projection then uses `.entries[]`; `--json entries` is not an admitted field.

Installed gh rejects `--paginate --slurp --jq` and `--slurp --template`. Use per-page `--paginate --jq`, or `--paginate --slurp | jq ...` for an aggregate. That parser refusal was reproduced without making a network request.

## Implemented changes

### Fair benchmark baseline

**What/where:** `packages/octocode-benchmark/scripts/guard/gh` rejected read-file/read-dir despite installed support; `prompts/gh-lean.md` omitted them and other efficient recipes. This disadvantaged the gh arm before any provider comparison.

**Fix:** permit those read operations, reject local-write `-o`/`--output`/`--clobber` forms, respect literal filenames after `--`, and consume option values so `--ref --` cannot bypass the guard. Teach server-side field selection, raw regions, correct pagination aggregation and a separate optional stale-cache tier.

**Evidence:** the new focused guard cases failed before the fix (six tests, eight failing subcases). The same six tests passed after it. Coordinator rerun: 6 passed in 4.028 s; independent rerun: 6 passed in 4.621 s. Shell syntax validation passed.

The existing guard still forbids clone and can reject a read-only GraphQL command merely containing `mutation`. Those restrictions are outside this minimal fix. They must not be scored as Octocode capability wins.

### Stable search-to-read identity

**What/where:** `gh_search_code/leads.rs` emitted an unversioned optional file read for paths-only results, although a validated indexed commit was already available. It was observed on both Vite and Cargo.

**Fix:** preserve an explicit requested ref, otherwise reuse the proven indexed commit. Unknown revisions remain unknown; blob identities are never used as commits. No provider request was added.

**Regression:** `path_only_handoffs_preserve_index_commits_without_using_blob_shas` covers owner/repository scope, known/unknown commits, requested-ref precedence, distinct blob identities and published lead validation. Live examples before the fix returned Vite `61622b8df68b7a8e657caa6f98d9b70069643abf` and Cargo `cc00aa41d556e87fd062adf7a8660f1cbc142c42`, yet their reads omitted `ref`.

### PR acquisition and representation

**What/where:** `gh_get_history_item/graphql.rs` required at least two requested pieces to use GraphQL, although one connection could share a request with the mandatory PR header. `graphql/document.rs` also fetched labels unused by section output.

**Fix:** admit a single eligible connection, retain body-only REST and existing patch/filter/page restrictions, and omit the unused labels selection. Under a successful complete one-page connection, the source-derived normal path changes from **2 REST requests to 1 GraphQL request**. There is no measured wall-clock improvement yet. Failure, extra pages and metadata fallback can add requests.

Independent review caught an important regression in the initial optimization: GraphQL's file nodes cannot preserve a renamed file's prior path. The final implementation checks representation completeness before and after connection paging. Renames, unsupported statuses and missing required file metadata leave **only that connection** to the existing REST fallback; the GraphQL header and other completed sections remain available. This also repairs the pre-existing multi-connection case. New tests cover old-path recovery and selective fallback.

Pure regression filters:

```text
path_only_handoffs_preserve_index_commits_without_using_blob_shas
a_single_connection_uses_graphql_but_body_only_and_filtered_files_do_not
graphql_file_metadata_requires_a_lossless_inventory_representation
section_headers_do_not_depend_on_unselected_labels
outgoing_documents_validate_against_the_github_schema
graphql_fast_path_requires_first_pages_and_no_patches
```

HTTP regression filters:

```text
one_requested_connection_and_its_header_use_one_graphql_request
graphql_files_missing_rename_metadata_use_rest_without_rereading_other_sections
renamed_patch_parent_read_uses_the_old_path
```

Native regression execution and a fresh rebuilt CLI/MCP check remain blocked. The first focused no-listener run reached compilation and failed with `E0599` at `crates/runtime/src/tools/lsp_search/syntax/facts.rs:136`: `CodeOccurrences` had no `ranges()` method. That concurrent LSP change was left untouched. After its source changed again, a second focused run was attempted and queued behind the Cargo lock; no native passing count is claimed. Formatting and whitespace checks passed. Independent source rereview found no further concrete regression in these changes; this is not runtime approval.

## Highest-value next work

| Priority | Action | Why / acceptance criterion |
|---|---|---|
| P0 | Run an unrestricted, same-context paired comparison. | Match credentials, binary build profile, refs, selected evidence, freshness, sorting and full page scope. Require correct output before counting a time win. |
| P1 | Use path projection for discovery, then fetch only selected declarations. | Avoid source work for a path question. Require source request counts to scale with the shown/selected files and all required continuations to remain reachable. |
| P1 | Verify the concurrent response-window source-acquisition change. | Current source already uses `gh_search_code/part.rs` and a deduplicated line plan to defer unrelated reads. This turn did not reimplement it, and measured deployment is unverified. |
| P1 | Push sections/fields into provider queries. | Keep the PR improvement conditional on evidence parity. Measure actual request count and fetched bytes, including failures and rename fallback. |
| P1 | Prefer persistent MCP or direct native execution for repeated work. | The Node wrapper adds measurable local startup cost. Validate a full workflow before claiming its startup difference as a task-level gain. |
| P2 | Retain ordered selected tree listings in continuation state if profiling justifies it. | Current source reranks/selects the pinned listing on later tool pages even when HTTP answers are cached. Target less repeated CPU, not an additional fresh-query result cache; preserve snapshot/auth bindings and existing storage budgets. |
| P2 | Reduce optional unrelated navigation after a workflow experiment. | The 718→445-character counterfactual is promising for a small fetch, but navigation can save agent effort. Preserve evidence, revision, required continuations and independently readable rows. |
| P2 | Consider combining default-branch name and OID acquisition. | Current metadata and HEAD resolution are concurrent, then tree acquisition follows. Fewer requests may reduce quota without eliminating the dependent acquisition phase. Preserve REST/enterprise fallback. |

A compact comparison should report **correct completed tasks, model-visible bytes, provider requests, fetched bytes, agent calls, elapsed time, CPU and peak memory**, with first-observed and warm repeats separate. Do not convert these exploratory samples into p95 or production claims.

## Verification and remaining scope

| Check | Status |
|---|---|
| Local startup samples | 15 sequential/interleaved samples completed; raw observations saved |
| Public connected-MCP probes | Selected Vite/Cargo scopes completed as recorded; broad unread pages named |
| Strong gh capabilities | Installed help and pinned upstream source inspected |
| Guard regression and syntax | Passed, including independent rerun |
| Independent native source review | Rename gap found, repaired, and rereviewed |
| Native pure regressions | First run failed on the concurrent LSP API mismatch; retry after the source changed queued on the Cargo lock |
| Native HTTP regression / fresh binary | Not executed; compilation blocker and sandbox socket restrictions |
| Rust formatting / whitespace | Pinned check of all 11 changed native files and scoped `git diff --check` passed |
| Documentation gate | Initial run passed; latest run fails four ignored-log links in the concurrent `2026-11-13-performance-cache-audit.md`, outside these report changes |
| Same-context remote gh latency/bytes | Blocked; no successful paired sample |
| Full-tool CPU/RSS, large-cache throughput, release footprint | Unmeasured |

Source areas edited this turn: the three benchmark files above; `gh_search_code/{leads.rs,code_output/tests.rs}`; and `gh_get_history_item/{graphql.rs,graphql/,graphql_tests.rs,graphql_tests/,pr_finish/metadata.rs}`. Other concurrent changes are preserved and are not attributed to this work.

## Primary sources

- [gh api](https://cli.github.com/manual/gh_api): pagination, caching, formatting and GraphQL parameters.
- [gh search code](https://cli.github.com/manual/gh_search_code): code search filters and text-match fields.
- [gh repo read-file](https://cli.github.com/manual/gh_repo_read-file) and [read-dir](https://cli.github.com/manual/gh_repo_read-dir): installed preview readers and output fields.
- [Pinned gh repository view implementation](https://github.com/cli/cli/blob/b300f2ec7ec9dc9addc39b2ad88c54097ded7ca0/pkg/cmd/repo/view/view.go#L114-L129): selected provider fields and omitted README.
- [Pinned gh API implementation](https://github.com/cli/cli/blob/b300f2ec7ec9dc9addc39b2ad88c54097ded7ca0/pkg/cmd/api/api.go#L254-L265): incompatible aggregate/projection flags; REST projection is after HTTP.
- [Pinned read-file implementation](https://github.com/cli/cli/blob/b300f2ec7ec9dc9addc39b2ad88c54097ded7ca0/pkg/cmd/repo/read-file/http.go#L32-L58) and [read-dir implementation](https://github.com/cli/cli/blob/b300f2ec7ec9dc9addc39b2ad88c54097ded7ca0/pkg/cmd/repo/read-dir/http.go#L102-L172): encoding, entry projection and object identity.
- [Pinned go-gh TTL cache](https://github.com/cli/go-gh/blob/a0a6e8947ae2ceedb496654757886ef41ef5ac72/pkg/api/cache.go#L55-L120): credential-bound key and reuse before origin contact.
- [GitHub REST best practices](https://docs.github.com/en/rest/using-the-rest-api/best-practices-for-using-the-rest-api): conditional requests and provider rate-limit behavior.
- [zsh timing parameters](https://zsh.sourceforge.io/Doc/Release/Parameters.html#Parameters-Used-By-The-Shell): elapsed/CPU time and maximum-memory fields.
