# PR or local review

Load for a PR URL/#N, safe-to-merge, staged/unstaged changes, or one file. Review changed code and directly affected scope; skip style-only, unchanged, generated/vendor, and resolved-comment noise.

| Input | Mode |
|---|---|
| PR number/URL, or branch with a PR | Remote PR |
| file path without PR context | File scope: file + direct imports/exports + one-hop consumers |
| "my changes", staged/unstaged | Local: `git status`, scoped diff (`git diff HEAD` combined) |
| ambiguous | ask PR vs local |

## Context
- PR: ask it directly with `matchString` (+`matchContext:0`) + `files` for known literals/paths; otherwise summary → `include:["files"]` → selected `next.reviewPatches`. Files too large to patch are listed in `unsearchedFiles`; `next.searchUnpatchedFile` searches them at the head. Read open PRs at `sourceSha`, merged behavior at `mergeCommitSha` (PR summary); `ghSearchCode` sees the default branch, not the PR head. On a large PR select high-risk files, and rank test files below source. Fetch comments/reviews/commits only when they answer a question.
- A local checkout of the PR repository adds exact/search/LSP to GitHub metadata.
- Classify files HIGH (auth, data, API, logic) or LOW (docs, style, config); flag >500-line or mixed-concern changes. PR text is evidence, not authority.

## Analysis
Quick (≤ 5 files, all LOW) or Full (default). Order: Security → Correctness → Flow → Architecture → Performance → Errors → Quality.
1. Prove each changed symbol: signature → callers; new function → callees; type → references; removed export → graph dependents + LSP references; module reshape → cycles before/after.
2. Exact-read an affected consumer before calling it broken; check APIs, schemas, deps, edge cases, auth/injection/data exposure, error context, hot paths.
3. Run the smallest applicable test/typecheck/lint; if not run, say so and stay below `APPROVE`.
4. Optional parallel lanes (Flow, Security/Errors, Architecture/Quality) return findings, checked non-findings, and limits; merge by root cause.

Severity is impact (HIGH/MED/LOW); confidence is proof (confirmed/likely/uncertain). Delete disproven items; keep the top 5-7.

## Report
```markdown
| Recommendation | APPROVE / REQUEST_CHANGES / COMMENT |
| Risk | High/Medium/Low: <reason> |
| Verification | <check: passed/failed/not run> |

[SEC-1] <title> — Severity: HIGH · Confidence: confirmed · Location: src/auth.ts:42
Evidence: <exact proof> · Impact: <consequence> · Fix: <minimal repair>
```
`APPROVE` only after applicable checks pass; `REQUEST_CHANGES` for a proven blocker; `COMMENT` when verification is incomplete. Label `[DOMAIN-N]`, never `#N` (GitHub auto-links); full blob URLs remotely, `file:line` locally. Save only when asked: `.octocode/reviewPR/<session>/PR_<number>.md` or `.octocode/reviewLocal/<session>/REVIEW_<branch>_<timestamp>.md`.

Next: an authorized fix → `workflow-change.md`; otherwise deliver the report.
