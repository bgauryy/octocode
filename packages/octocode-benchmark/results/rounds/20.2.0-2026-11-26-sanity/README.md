# Octocode 20.2.0 sanity round: every tool, schemas and workflows, 26 November 2026

All 17 tools answered correctly on the build with tool sections and gated description clauses. No wrong answer, lost evidence or unreachable page was found, so no score is capped.
The scores are provisional: this round measured correctness, schema and input, output, paging and workflow,
not efficiency against a rival, timing or freshness.

| Tool | Score | Correct | Schema/input | Output | Paging | Workflow | Measured weight | Confidence | Main deduction |
|---|---:|---:|---:|---:|---:|---:|---:|---|---|
| localFetch | 9.5 | 10 | 9 | 9 | 10 | 9 | 80% | medium | states the checkout's commit for a file Git ignores |
| localSearch | 9.5 | 10 | 9 | 9 | 10 | 9 | 80% | medium | long repeated paths in rows |
| structureSearch | 9.6 | 10 | 9 | 9 | 10 | – | 70% | medium | no workflow covered it |
| astSearch | 9.3 | 9 | 9 | 10 | – | – | 60% | low | one rule tried; paging not walked |
| lspSearch | 8.4 | 8 | 9 | 9 | – | 8 | 70% | medium | the TypeScript server left one use out (disclosed, with a lead) |
| ghSearchRepo | 9.5 | 10 | 9 | 9 | – | – | 60% | low | `get` only |
| ghSearchCode | 9.4 | 9 | 9 | 10 | – | 10 | 70% | medium | one repository, one term |
| ghStructure | 9.3 | 10 | 9 | 8 | 10 | 9 | 80% | medium | three row shapes in one `files` list |
| ghGetFileContent | 9.6 | 10 | 9 | 9 | – | 10 | 70% | medium | paging not walked |
| ghSearchHistory | 9.3 | 10 | 9 | 8 | 10 | 9 | 80% | medium | a merge commit is an object among string rows |
| ghGetHistoryItem | 9.1 | 9 | 9 | 9 | – | 10 | 70% | medium | patch paging not walked |
| artifactSearch | 9.8 | 10 | 9 | 10 | – | 10 | 70% | medium | npm `resolve` only |
| securityCheck | 9.4 | 10 | 9 | 9 | – | 9 | 70% | medium | `package` only |
| ghCloneRepo | 9.1 | 9 | 9 | 9 | – | 10 | 70% | medium | one small repository |
| clasify | 8.2 | 8 | 8 | 9 | – | 8 | 70% | low | one question; the largest schema (8.7 KB) |
| graphBuild | 8.0 | 8 | 8 | 8 | – | – | 60% | low | counts not checked against a source oracle |
| graphQuery | 8.6 | 9 | 9 | 8 | – | 8 | 70% | low | two operations of 21 |

Scores use the weighted rubric of [RATINGS.md](../../../RATINGS.md) over the measured dimensions only; "–" is
not measured, not inapplicable. A dimension checked with a single probe scores at most 9.

## What each tool was checked against

| Tool | Call | Oracle | Result |
|---|---|---|---|
| localFetch | lines 4–9 of the fixture's `calls.ts`; the whole 614-line `long.ts` in 1,500-character pages | `sed`; the file itself | exact; 25 pages rebuild every line once, in order, Unicode intact |
| localSearch | count `beacon` in the fixture; 60 matched lines in pages of 7 | `rg -c`, `rg -o` | 91 matches on 61 lines in 2 files, as `rg`; 9 pages hold 60 lines once |
| structureSearch | the three largest files; all entries in pages of 4 | `find` with sizes | same three, same order; 18 of 18 entries |
| astSearch | `runTask($A)` calls in `calls.ts` | the source | 2 calls; the comment and string decoys are not matched; a diagnostic names the receiver call on line 7 |
| lspSearch | references and callers of `runTask` | the source | lines 3, 5, 6, 9; line 7 is left out by the server and the reply says so (`serverUnanswered`) with a text search to read it |
| ghSearchRepo | `get` expressjs/express | `gh api repos/…` | 69,753 stars, `master`, JavaScript: equal |
| ghSearchCode | `createApplication` in expressjs/express | the file | `lib/express.js`, lines 24, 27, 36 and the declaration's end |
| ghStructure | `lib` listing; whole tree to depth 3 in pages of 40; tags | `gh api …/contents`, `…/git/trees?recursive=1` | 6 of 6 files; 162 of 162 files over 6 pages; newest tag `v5.3.0` |
| ghGetFileContent | `createApplication` with `block`; `view: symbols`; `package.json` at a release SHA | the file | lines 36–56; the outline; `"version": "3.22.4"` |
| ghSearchHistory | commits of `lib/express.js` in pages of 10; merged pull requests | `gh api …/commits?path=` | 158 of 158 commits over 16 pages; 584 merged in total |
| ghGetHistoryItem | PR 6000 and 7580 with files and reviews; compare `v5.2.1`…`v5.3.0` for `package.json` | `gh pr view`, `gh api …/compare` | titles and states equal; 76 commits ahead, as `gh`; one filtered patch |
| artifactSearch | resolve `zod@3.22.4` | `npm view … gitHead` | `colinhacks/zod` at `e7a9b9b…`, tag `v3.22.4`: equal |
| securityCheck | `lodash@4.17.20` and `4.17.21` | OSV `v1/query` | the same four advisory ids; the withdrawn one is named and not counted |
| ghCloneRepo | clone expressjs/express, then three searches in one call | `rg` in the checkout | counts equal; `.npmrc` reported as skipped |
| clasify | offline preview; one yes/no question on `calls.ts` | reading the file | 0.89 yes for lines 1–3; one billed request; `next.read` offered |
| graphBuild | `packages/octocode-mcp` | – | 9 files, 58 nodes, 88 edges; types with no grammar are named |
| graphQuery | `dependents` of `startup-failure.ts`; `issues` | `grep` for the import | `src/index.ts`; `issues` is empty at the default tier and says three findings are hidden, with the call that lists them |

## Schemas

| Check | Result |
|---|---|
| `schema <tool>` views `query`, `output`, `variants` and `--help` | served for all 15 tools of the main CLI; the two graph tools through `octocode-graph` |
| Each tool's section in `octocode schema` | 7 external, 6 local, 1 security (clasify appears with a key) |
| MCP output reference `octocode://contract/output/<tool>` against `schema <tool> --view output` | equal for all 14 tools listed |
| MCP description against the CLI's | equal for all 14 after a fix made in this round: the CLI did not drop a gated clause |
| One near-miss call per tool (a sibling's field name, a missing field, a near enum value) | all 17 refused with exit 2, the real field or value named, and a runnable example; no provider call |
| MCP `tools/list` with no setting | 7 tools, the external section |
| A tool whose section is off, called over MCP | refused: "it needs local.enabled (OCTOCODE_ENABLE_LOCAL)", "it needs tools.enableSecurity (OCTOCODE_ENABLE_SECURITY)" |
| The earlier security variable | `OCTOCODE_SECURITY_CHECKS=true` lists `securityCheck`, as `OCTOCODE_ENABLE_SECURITY=true` does |
| CLI = MCP answers | identical JSON for one localFetch and one ghSearchCode call |
| Redaction | a GitHub token in a file reads `[REDACTED-GITHUBTOKENS]` in localFetch and localSearch, only the secret part |

## Workflows

Each finished in the calls [CHECKS.md](../../../CHECKS.md#workflows-each-must-finish-in-the-same-number-of-calls-as-last-round) expects.

| Workflow | Calls | Result |
|---|---:|---|
| Search a symbol, read its body | 2 | pass |
| Folder, then an outline | 2 | pass |
| A file's history, then the newest patch | 2 | pass |
| A merged PR, then its files and reviews | 2 | pass |
| A tag, then one file's change between releases | 2 | pass |
| A package version, then its source at that release | 2 | pass |
| Is this version affected | 1 | pass |
| Many searches in one repository | 2 (clone, then one call with three searches) | pass |
| Local: find, then read | 2 | pass |
| Local: who calls it | 2 | pass, once the call named a `workspaceRoot` (see limits) |
| Local: what is unused | 2 | pass; empty at the default tier, with the lead to the hidden findings |
| Large unread evidence | 1 | pass |

## Limits of this round

| Limit | Effect |
|---|---|
| One reviewer, direct calls | Not a blind agent trial; agent understanding was not tested |
| Efficiency, timing and freshness not measured | 20–30% of each score's weight is missing; every score is provisional |
| One or two operations per tool | Other operations and views are unverified, most of all for graphQuery, clasify and securityCheck |
| No stress cases | Huge scopes, multi-megabyte lines and provider failures were not tried |
| The test home sat inside the workspace | After the clone, `lspSearch` refused the workspace as holding fetched code; the message named the cause and `workspaceRoot` fixed it. In a normal home this does not arise |
| One wrong oracle, corrected | My first tree comparison missed 11 files that ghStructure lists as whole-path strings beside its folder groups; the file-by-file recount is 162 of 162 |

## Round identity

| | |
|---|---|
| Build | `octocode 20.2.0+08808283`, SHA-256 `c8d336ec286c…` (dev profile), pinned under `.octocode/octocode-dev/pins/` |
| Tree | commit `e3ed53555` with uncommitted changes: tool sections and gated clauses. The round ran while setting variables were briefly unprefixed (`ENABLE_LOCAL`); they are `OCTOCODE_`-prefixed again and that change was not re-run through this round |
| Surfaces | CLI (`packages/octocode/out/octocode.js`) and MCP (`packages/octocode-mcp/dist/index.js`, stdio) |
| Local fixture | `campaigns/20.2.0-2026-11-14-tool-ratings/local/fixture` |
| GitHub targets | `expressjs/express` at `43fca52d…`, `colinhacks/zod` at `e7a9b9b3…` |
| Gates on this tree | Rust suite 9,615 passed; core 2,035; config 190; docs check passed |
