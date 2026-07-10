# Workflow: PR Or Local Review

Load for PR URL/#N/safe-to-merge, local staged/unstaged review, or a specific file. Review changed code/direct blast radius; skip style-only, unchanged, generated/vendor, and resolved-comment noise.

## Tool And Target Rules
- Prefer Octocode MCP/CLI for code search/read/analysis; shell is limited to `git status/diff/log/branch` when Octocode is available.
- Continue with stated reduced coverage when Octocode is missing. Never guess file content; each nontrivial call supports a ledger hypothesis.

| Input | Mode |
|---|---|
| PR number/URL or branch with PR context | Remote PR |
| file path without PR context | Local File Scope |
| “my changes/diff”, staged/unstaged/local | Local Changes |
| ambiguous | ask PR target vs local changes |

## Availability
- PR: metadata/changed files resolve. Ask for a corrected target only on not-found; route auth/rate/transport failures through `octocode.md` and report degraded/blocked coverage.
- Local Changes: local tools and `git status` work; at least one staged/unstaged/untracked change exists.
- File Scope: the file exists; File Scope does not require staged, unstaged, or untracked changes. Inspect it plus direct imports/exports and one-hop consumers.
- LSP failure is not absence; use exact/structural/text proof.

## Guidelines
Discover `.octocode/pr-guidelines.md`, context docs, `CONTRIBUTING.md`, and `AGENTS.md`; ask once for additional path/inline guidance or “skip.” Record source/precedence and carry rules into findings. Explicit user guidance wins, then project guidance, repo context, domain defaults, style.

## Context
**PR:** fetch metadata/changed files, open review/discussion comments, commits, and selected high-risk patches. Use all patches only for small PRs; past ~2000 changed lines stay selected and start high-risk.

**Local:** collect status, scoped staged/unstaged diff, recent log/branch, changed symbols, and parent structure. Use `git diff HEAD` only for combined scope; ask to narrow an oversized diff.

Both: classify files HIGH (auth/data/API/logic) or LOW (docs/style/config); group by functional area; flag >500-line or mixed-concern changes.

## Checkpoint And Tool Routing
Before Full analysis, present scope/areas, staged state, risk, sizing, guidelines, and early concerns; ask focus unless tiny/LOW. Wait when focus changes scope.

| Mode | Code proof |
|---|---|
| PR repo is local | local exact/search/LSP + GitHub metadata/comments |
| remote-only PR | GitHub tree/search/exact/history; package metadata for dependency claims |
| Local/File | local exact/search/LSP + shell git context |

Search/patch hits lead to exact reads; exact anchors lead to callers/references/callees. Follow `references/workflow-pr-review-analysis.md` for sizing, flow proof, findings, and verification; then `references/workflow-pr-review-report.md` for recommendation/output. Validate with `node scripts/eval-research.mjs --case pr-local-review`.
