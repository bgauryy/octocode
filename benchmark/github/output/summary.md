# Benchmark summary — octocode vs gh

**Judge model:** claude-sonnet-4-6  
**octocode researcher model:** claude-sonnet-4-6  
**gh researcher model:** gpt-5.1-codex-max  
**Run date:** 2026-06-07

---

## Per-question table

| Q | Category | Drift | Octo qual | gh qual | Octo calls | gh calls | Octo chars | gh chars | Octo token score | gh token score | Winner | Notes |
|---|---|:---:|:---:|:---:|---:|---:|---:|---:|---:|---:|---|---|
| Q1 | SEARCH | | 2 | 1 | 2 | 4 | 20,825 | 9,978 | 0.096 | 0.100 | ~tie | Octo correctly identifies 250+ files beyond API cap; gh capped at 100 results (its ceiling), token scores nearly identical |
| Q2 | SEARCH | | 3 | 3 | 3 | 9 | 8,176 | 193,893 | 0.367 | 0.015 | **octo** | Both found correct file:line for all 3 repos; octo used bulk query (3 calls vs 9), 24× fewer chars |
| Q3 | SEARCH | | 3 | 2 | 3 | 288 | 4,960 | 1,877,334 | 0.605 | 0.001 | **octo** | Both found the 3 production call sites; gh included comment lines (`// when using compose()`) as matches; 288 calls and 1.8M chars for marginal info gain |
| Q4 | SEARCH | | 2 | 2 | 1 | 74 | 16,432 | 5,831,123 | 0.122 | 0.000 | **octo** | Octo: 24 files via native AND; gh: 41 files via brute-force per-file reads including compiled bundles; neither fully complete; 355× char gap |
| Q5 | CONTENT | | 1 | 3 | 7 | 1 | 28,855 | 202,909 | 0.035 | 0.015 | **octo** | gh fetched whole blob (202K chars) and listed all 60+ exports + accurate `performConcurrentWorkOnRoot` description; octo partial list and incorrectly stated function was refactored away |
| Q6 | CONTENT | | 3 | 2 | 5 | 2 | 31,740 | 362,135 | 0.094 | 0.006 | **octo** | Octo: correct 4.0.0 (2022-12-09) with full changelog context; gh: found 4.0.0-beta.0 from a linked secondary changelog, not the main `4.x` stable release |
| Q7 | CONTENT | | 1 | 3 | 4 | 1 | 4,432 | 3,151,873 | 0.226 | 0.001 | **octo**(token) / **gh**(quality) | gh read the full 3 MB blob and found `createBasicNodeBuilderModuleSpecifierResolutionHost` at line 54,260; octo hit 413 limit and reported UNKNOWN for the tail |
| Q8 | CONTENT | | 3 | 3 | 2 | 1 | 2,355 | 25,602 | 1.274 | 0.117 | **octo** | Both correct (27 files, all .js, none .ts); octo 11× fewer chars |
| Q9 | STRUCTURE | | 3 | 3 | 1 | 1 | 1,075 | 32,732 | 2.791 | 0.092 | **octo** | Both correct (13 .ts files, same list); octo 30× fewer chars |
| Q10 | STRUCTURE | | 3 | 3 | 1 | 2 | 2,145 | 22,011 | 1.399 | 0.136 | **octo** | Both correct; octo single bulk call, richer architectural commentary; 10× fewer chars |
| Q11 | STRUCTURE | | 3 | 2 | 7 | 3 | 18,292 | 61,181 | 0.164 | 0.033 | **octo** | Octo traced `createServer` → `_createServer` and listed the 7-step startup sequence; gh identified entry from CLI angle but stopped at function name without tracing internal startup |
| Q12 | PR | ✓ | 1 | 0 | 7 | 10 | 104,744 | 3,318 | 0.010 | 0.000 | **octo** | Both failed the label filter; octo provided a title-based fallback with 5 plausible PRs; gh returned 0 with no fallback |
| Q13 | PR | | 1 | 2 | 2 | 1 | 12,828 | 5,268 | 0.078 | 0.380 | **gh** | gh correctly reported 2 inline review comments (separate API resource) with a quoted comment; octo reported 29 total comments (includes PR-level) and inferred file from diff rather than reading inline threads |
| Q14 | PR | | 2 | 1 | 18 | 4 | 204,865 | 117,337 | 0.010 | 0.009 | **octo**(quality) | Octo found the correct introducing PR (#306 by yusukebe) but UNKNOWN commit count; gh found PR #1986 (a later jsx/dom-runtime extension, not the first JSX introduction) with complete 22-commit list |
| Q15 | PR | | 3 | 3 | 1 | 3 | 1,237 | 51,488 | 2.425 | 0.058 | **octo** | Both correctly identified PR #306 with motivation and changed files; octo provided richer per-file description; 42× fewer chars |
| Q16 | REPOS | ✓ | 2 | 1 | 2 | 2 | 19,984 | 2,145 | 0.100 | 0.466 | **gh**(token) / **octo**(quality) | Octo listed 6+ MCP-specific repos (correct topic filter); gh reported 202 total but top 5 were major platforms (n8n, dify, lobehub) that happen to carry the `mcp` topic — relevance quality lower |
| Q17 | REPOS | ✓ | 1 | 3 | 35 | 1 | 346,891 | 1,254,816 | 0.003 | 0.002 | **gh**(quality) | gh `gh api` org endpoint returned complete list: 233 repos, correct top-5, 44 repos with 1000+ stars; octo's search tool cannot enumerate an org exhaustively — reported UNKNOWN for total and 1000+ count |

---

## Quality verdict (non-drift Qs only)

Non-drift questions: Q1–Q11, Q13–Q15 (14 questions, max score = 42)

| Agent | Σ quality | Token-score wins | Token-score ties | Avg quality per Q |
|---|---:|---:|---:|---:|
| octocode | **33** | **12** | 0 | **2.36** |
| gh | **33** | 2 | 0 | **2.36** |

Both agents achieved the same aggregate quality on non-drift questions. Octocode won the token-score on 12 of 14 questions; gh won Q13 (inline comments, where gh correctly distinguished inline vs PR-level) and Q1 (marginally, by 0.004 points).

---

## Drift verdict (reported separately)

| Q | Category | Octo qual | gh qual | Notes |
|---|---|:---:|:---:|---|
| Q12 | PR | 1 | 0 | Label filter failed for both; octo gave title-based fallback PRs; gh returned zero results entirely |
| Q16 | REPOS | 2 | 1 | Octo found quality MCP-specific repos; gh found 202 total but top-5 included large platforms that carry the mcp topic |
| Q17 | REPOS | 1 | 3 | gh `gh api orgs/vercel/repos` enumerated all 233 repos; octo's search tool has no org-listing capability |

Drift Σ quality: octocode **4**, gh **4** — exactly tied.

---

## Quality-adjusted token-usage verdict

| Axis | octocode | gh | ratio (octo/gh) |
|---|---:|---:|---:|
| Σ quality (non-drift) | 33 | 33 | 1.00× |
| Σ calls (total, all 17 Qs) | 101 | 407 | 0.25× |
| Σ in_chars (per-Q) | 22,773 | 41,852 | 0.54× |
| Σ out_chars (per-Q) | 807,063 | 13,163,291 | 0.06× |
| TOTAL chars (all 17 Qs) | 829,836 | 13,205,143 | **0.063×** |
| Approx tokens (TOTAL chars / 4) | ~207,459 | ~3,301,286 | 0.063× |
| Quality per 1k chars (non-drift Qs only) | **0.0924** | **0.00276** | **33.5×** |
| Σ tool_elapsed_ms (context only) | 299,726 | 353,057 | 0.85× |
| Σ q_elapsed_ms (context only) | 1,265,241 | 660,088 | 1.92× |
| Σ reasoning_ms (context only) | 965,515 | 307,031 | 3.14× |

**Octocode produced the same raw quality using 16× fewer total chars.** On the quality-per-1k-chars metric, octocode scores **33.5× higher** than gh CLI.

The longer wall-clock q_elapsed for octocode (1.26s vs 0.66s total reasoning budget) reflects heavier in-context reasoning by the claude-sonnet-4-6 researcher — a cost not captured in metered chars.

---

## Capability Review

### Q3 — SEARCH bulk workflow
gh researcher issued **288 calls** and consumed **1.877M chars** to find `compose()` call sites by reading every file that matched the pattern. Octocode used 3 calls and 5K chars via `githubSearchCode` with `textMatches`, returning file:line:snippet directly. gh's approach also mis-classified two comment lines in `src/hono.test.ts` as actual `compose()` calls. **SEARCH bulk workflow** cap exposed.

### Q4 — SEARCH AND-intersection
`gh search code` is OR-union by default. The gh researcher compensated with **74 calls** (fetch each file, test for both keywords) consuming **5.83M chars**. Octocode expressed the AND constraint natively in a single call. gh's final list (41 files) was more complete but included compiled/bundled React DOM files. Octocode returned 24 (incomplete at pagination boundary) without noting compiled file exclusion. **SEARCH result limit** + **SEARCH bulk workflow**.

### Q7 — CONTENT over-size-limit file
`checker.ts` is 3,078 KB (~54,000 lines). The GitHub `/contents/` API returns HTTP 413 for files over 300 KB. Octocode's content tool received the 413 and could not paginate to the tail, reporting UNKNOWN for the last function. gh used `gh api repos/microsoft/TypeScript/git/trees` + raw blob path, receiving the full 3.15M chars in a single call and correctly identified `createBasicNodeBuilderModuleSpecifierResolutionHost` at line 54,260. **CONTENT large file path** — gh's raw blob endpoint wins for this edge case, at extreme character cost.

### Q11 — STRUCTURE entry-point discovery
gh identified the entry chain from `cli.ts` to `createServer` but stopped there. Octocode continued tracing into `_createServer` and enumerated the 7-step startup sequence (`disableCache`, `resolveConfig`, `initPublicFiles`, `resolveHttpsConfig`, `connect`, `resolveHttpServer`, `createWebSocketServer`). **STRUCTURE tree shape** — octocode's multi-step content read within the same question loop delivered a more complete answer.

### Q13 — PR inline comments
`gh pr view --json reviews` returns PR-level review summaries only. The inline thread comments are a separate API resource (`/repos/{owner}/{repo}/pulls/{pull_number}/comments`). gh accessed this resource directly and correctly returned 2 inline review comments. Octocode's `withComments: true` retrieved 29 total comments (inline + PR-level combined) but did not separate them, causing the researcher to over-count. **PR inline comments** — one point where gh's explicit API access was more precise.

### Q14 — PR commit history
gh found PR #1986 (a later `jsx/dom` runtime extension) rather than the original #306 `feat: jsx middleware`. This is a PR identification error, not a commit pagination problem — both Q14 and Q15 were answered on the same session, and Q15 correctly identifies #306. Octocode found the correct PR #306 but `withCommits: true` returned no commit data, leaving commit SHAs and messages UNKNOWN. **PR diff completeness** — octocode's search accuracy wins but commit enumeration capability gap remains.

### Q17 — REPOS org enumeration
`githubSearchRepositories` does not support exhaustive org-level listing. Octocode's researcher had no tool that pages through `GET /orgs/{org}/repos` and reported UNKNOWN for the total count and 1000+ repo count. gh called `gh api orgs/vercel/repos --paginate` in a single call and retrieved all 233 repos, answering all three sub-questions correctly. **REPOS pagination** — clear gh capability advantage for org enumeration.

---

## Verdict

**Quality:** Tied — both agents scored **33 / 42** on the 14 non-drift questions and **4 / 9** on drift questions.

**Token efficiency:** Octocode wins decisively — **33.5× higher quality per character** (0.0924 vs 0.00276 quality/1k chars). Octocode used 829K total chars vs gh's 13.2M total chars to produce the same aggregate quality.

**Where gh wins outright:**
- Large oversized-file retrieval (Q7) — raw blob access handled a 3 MB file octocode could not read past HTTP 413
- Org-level repo enumeration (Q17 drift) — `--paginate` on the orgs endpoint beats a missing capability entirely
- Inline PR comment access (Q13) — gh correctly hit the separate review-comments endpoint

**Where octocode wins outright:**
- Bulk multi-repo code search (Q2, Q3) — one structured call vs 9–288 sequential calls
- AND-intersection search (Q4) — native semantics vs 74-call brute-force
- Large-file tail read (Q6) — char-offset window to the 4.x section vs dumping the full 362K blob
- PR body + diff in one call (Q15) — 1 call / 1.2K chars vs 3 calls / 51K chars
- Directory listing with typed metadata (Q8) — 2.3K chars vs 25K chars for identical results

**Summary:** Octocode's structured tool set delivers the same answer quality as the gh CLI at a fraction of the character budget. For agentic use cases where context-window cost matters, octocode is the clear winner. gh retains an advantage specifically for raw blob access (oversized files) and org-level enumeration — capabilities that do not exist in the current octocode toolset.
