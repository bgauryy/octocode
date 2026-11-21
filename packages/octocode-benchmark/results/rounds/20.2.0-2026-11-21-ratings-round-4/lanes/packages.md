# Round 4 — Packages lane (artifactSearch, securityCheck)

Binary: `packages/octocode-native/npm/darwin-arm64/octocode`, `octocode 20.2.0+56f67471`, SHA-256 `e246ff42592b8192da65589ef39a7f573f04f4643b15765e9d98fb5cff6a405f`, built 2026-11-21 00:00:49; HEAD `e9309aa35`, dirty tree (97 entries). CLI `--json` only.
Ground truth: crates.io, registry.npmjs.org (+ `/-/v1/search`), pypi.org JSON, rubygems v2, api.nuget.org, proxy.golang.org, central.sonatype.com solrsearch, api.osv.dev `/v1/query` and `/v1/vulns`, `gh api` GET (tags → commits, repo + global security advisories).
Spend: ~45 tool calls (incl. 2 page walks of 127 and 12 pages), ~0.9 MB output read programmatically; ~35 min.

## Scores

| Tool | Desc | Schema | Workflow | Clarity | Time | Tokens | Paging | Smart | Short | Long | Stress | Hints | Overall | Correct |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| artifactSearch | 9 | 9 | 9 | 9 | 7 | 9 | 9 | 9 | 9 | 8 | 8 | 8 | **8.5** | 10 (24/24 refs, dates, versions) |
| securityCheck | 9 | 8 | 9 | 8 | 8 | 7 | 9 | 9 | 9 | 8 | 8 | 8 | **8.5** | 9.5 (counts/upgradeTo right on 19 packages; one misleading summary, P1) |

- `in: manifest/lockfile, ghSearchCode → artifactSearch → out: ghStructure tree at ref, securityCheck (related call), ghGetHistoryItem compare`
- `in: artifactSearch related, manifest → securityCheck → out: securityCheck advisory details (withdrawn leads), artifactSearch, ghStructure`

Times: median 2.7 s per call; cold 7.2 s vs warm 2.0 s (artifactSearch), 2.3 vs 1.2 s (securityCheck); npm 2-keyword search cold 19.3 s.

## What was verified (all against external sources)

artifactSearch resolve — every ref equals `gh api repos/<o>/<r>/commits/<tag>`: tokio-util 0.7.10 (monorepo dir `tokio-util`, tag `tokio-util-0.7.10`), @tanstack/react-query 5.28.0 (dir `packages/react-query`, matches npm `repository.directory`), pydantic-core 2.16.3 (name normalized `pydantic_core`), jackson-databind 2.16.1, symfony/console v6.4.4, nokogiri 1.16.2, cobra v1.8.0, Serilog 3.1.1, Django `>=4.2,<4.3` → 4.2.30 (pypi newest 4.2.x), express `^4.17.0` → 4.22.3 (provenance, gitHead match), typescript `next` → 7.1.0-dev.20261120.1 (npm dist-tag, gitHead match), left-pad (rename warning + ghStructure lead on new owner), testify/assert v1.9.0, zap v1.27.0, golang/protobuf v1.5.3 (deprecated, date = proxy), gin/binding v1.9.1, requests 2.31.0 details (3 advisories = PyPI's 6 ids alias-deduped). publishedAt matches registry on every row that has it. serde 9.9.9 → notFound with 5 resolve leads starting at crates.io max_stable 1.0.229.

artifactSearch search — Maven `jackson` 3 pages × 40 = 100 rows, identical order to Central `rows=100`, cap stated (444 total, 100 reachable). npm `tokenizer bpe`: 12 pages, 48 unique rows, all 16 AND-matches in npm's first 250 present; 5 sampled extras contain both words. crates/nuget/packagist/rubygems searches return sensible rows with totals and cap warnings.

securityCheck — advisoryIds equal OSV live ids modulo aliases (checked each missing id's aliases) for log4j-core 2.14.1 (8), axios 0.21.1 (24), jinja2 2.11.2 (5 of 10 = aliases), hyper 0.14.10, golang.org/x/net v0.7.0 (24/30 aliases), rack 2.2.3 (35), urllib3 purl 1.26.4 (11/22 aliases), time 0.1.43 URL, rustdecimal, rest-client 1.6.13, Debian 11 openssl (9), lodash (4), Pillow 9.0.0 (54/88 aliases), Newtonsoft.Json 12.0.1, guzzle 7.4.4 (11). upgradeTo equals the OSV-derived minimum clean version everywhere; axios 0.34.0 and lodash 4.18.1 are right *because* 0.33.0 / 4.18.0 carry other advisories (checked on OSV) — smart. Repository express: 2 counted + 1 withdrawn matches `gh api advisories/GHSA-pj86-cfqh-vqx6` withdrawn_at (repo endpoint does not show it). Advisory CVE-2021-44228 resolves to GHSA alias with ranges. Commit unknown → honest "proves nothing".

Paging stress: rack details at `responseChars:3000` → 127 parts; rebuilt with `rowPart.continues` (incl. nested pointer `/data/advisories/4/affected`) equals the single-page row exactly, except repeated identity fields (`packageName`, `version`) that the contract says to keep once. Overhead 312 KB vs 251 KB (+24%).

Rate limit: npm HTTP 429 mid-walk returns `rateLimited`/`transient` with `next.retry`; the retry cursor resumed correctly after 60 s.

## Defects (most severe first)

| # | Tool | Finding | Exact input | Observed vs expected | Evidence | Sev | Owner |
|---|---|---|---|---|---|---|---|
| R4-P1 | securityCheck | Go package path inside a module: the summary names the **module** and says none known, and the warning says OSV does not list an advisory that OSV lists for that module | `{"operation":"package","ecosystem":"Go","packageName":"github.com/gin-gonic/gin/binding","version":"v1.9.0"}` | answer `github.com/gin-gonic/gin@v1.9.0: none known to OSV`, `packageName: github.com/gin-gonic/gin`, warning `GitHub ... matches GHSA-2c4m-59x9-fr2g ... OSV does not list it`. Expected: name the package `.../binding`, say the module has 1 advisory scoped to package `github.com/gin-gonic/gin` (as `moduleOnly` in details does); summary has no `moduleOnly` lead | OSV `/v1/query` gin v1.9.0 → GHSA-2c4m-59x9-fr2g, GO-2023-1737; GO-2023-1737 `imports` path `github.com/gin-gonic/gin` only. Same tool on the module path → affected, upgradeTo v1.9.1 | med | native (+core text) |
| R4-P2 | securityCheck | details view repeats each advisory once per database as separate array elements with the same id; 35 advisories → 98 elements, 251 KB for one package | rack 2.2.3 `view:"details"` | expected one element per advisory with per-source facts, or a source label; observed 35 OSV + 35 GitHub-global + 28 repo-advisory copies, differing only by which of `source` / `link`+`githubType` / `link`+`publisher` is present; severities disagree (moderate / medium / low) with no field naming the database | rebuilt row; `sources[].facts.repositoryAdvisories` lists only 1 id while 28 repo copies appear | low-med | native + core |
| R4-P3 | securityCheck vs artifactSearch | `@Angular/core`: artifactSearch now leads to `@angular/core`, securityCheck still says notFound with a search lead on `@Angular` | `{"operation":"package","ecosystem":"npm","packageName":"@Angular/core","version":"11.0.0"}` | expected the same lowercase lead | artifactSearch hint `npm package names are lowercase: retry as @angular/core.` | low | native |
| R4-P4 | artifactSearch | Go package paths inside a module drop `publishedAt` | `github.com/stretchr/testify/assert` v1.9.0; `github.com/gin-gonic/gin/binding` v1.9.1 | module rows carry it; proxy `.info` Time 2024-02-29T14:36:18Z | proxy.golang.org | low | native |
| R4-P5 | both | No lookalike lead for a missing PyPI name (`reqeusts`), though crates `rustdecimal` gets a lookalike caution | artifactSearch resolve / securityCheck package PyPI `reqeusts` | expected lead to `requests` | — | low | native |
| R4-P6 | artifactSearch | npm multi-keyword search: cold 19 s for 5 rows; short/empty pages (1–16 rows at limit 20) burn the npm rate limit (429 after ~10 pages); retry hint gives no wait time | `{"operation":"search","ecosystem":"npm","keywords":["markdown","parser"],"limit":5}` | documented in `limit` text; still costly | calls.log | low | native |

## Regression check (round 3)

| Id | Status | Evidence |
|---|---|---|
| R3-P1 removed packages never sent to OSV | **fixed** | PyPI `afriton` → MAL-2024-11514 + malware caution, `registryConfirmed:false`; crates `rustdecimal` 1.23.1 → GHSA-7pwq (aliases MAL-2022-1, RUSTSEC-2022-0042) + malware + lookalike cautions |
| R3-P2 yanked RubyGems version not evaluated | **fixed** | rest-client 1.6.13 → 3 advisories (= OSV), upgradeTo 1.8.0, `versionListed:false` + do-not-install warning |
| R3-P3 Go subpackage loses owner/repo, contradictory warning | **fixed** (artifactSearch); new P4 (no publishedAt) and P1 (securityCheck text) | testify/assert, gin/binding rows carry owner/repo/modulePath/tag ref |
| R3-P4 vanity modules `registryRef` | **fixed** | go.uber.org/zap v1.27.0 `verification:tag`, ref = GitHub tag |
| R3-P5 distro packages never get upgradeTo | **fixed** | Debian 11 openssl → upgradeTo `1.1.1w-0+deb11u8` (= OSV ranges), 9 advisories |
| R3-P6 spaced version called a range | **fixed** | lodash `" 4.17.20 "` → checked as 4.17.20 |
| R3-P7 smaller | **partly fixed** | fixed: `@Angular/core` lead in artifactSearch, golang/protobuf deprecated (both tools); not fixed: `reqeusts` lookalike, securityCheck `@Angular` lead, bare Maven name gets text "artifactSearch search first" but no related call; `mainGoal` >500 still rejects the call (schema maxLength 500, so by contract); `oversized` at 2500 not re-tested |

## Good

- upgradeTo is checked against the target itself (axios 0.34.0 not 0.33.0; lodash 4.18.1 not 4.18.0), and withdrawn advisories are excluded with a lead — more accurate than the GitHub repo-advisory endpoint.
- rowPart continuation with nested `continues` pointers rebuilds a 251 KB row exactly; Maven/npm walks are duplicate-free and caps are stated.
