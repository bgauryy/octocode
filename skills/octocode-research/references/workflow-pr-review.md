# Workflow: PR Or Local Review

Use for `review PR`, `safe to merge`, `review my changes`, staged/unstaged diffs, and local file-scope review. Read `algorithm.md` first for the router and evidence grades; use `code-research.md` for the proof ladder on any finding.
Review changed code and direct blast radius; avoid style-only, unchanged-code, generated/vendor, and already-commented findings unless unresolved.

PR review workflow uses Octocode's real tools: `ghHistoryResearch`, `npmSearch`, `lspGetSemantics`, and `ghViewRepoStructure`.

## Target Detection

| User input | Target | Mode |
|---|---|---|
| PR number ("review PR #123") or PR URL | Remote PR | PR Mode |
| Branch name with PR context | Remote PR | PR Mode |
| Specific file path, no PR context | Local file | Local Mode (File Scope) |
| "review my changes/diff", "review staged/unstaged changes", "review local changes" | Local changes | Local Mode |
| No PR specified, "review my code" | Local changes | Local Mode |
| Ambiguous | — | Ask: "Would you like me to review a specific PR or your local changes?" |

File Scope: verify the file exists before anything else; if missing, stop and ask for the correct path.
If it exists, inspect the file, direct imports/exports, and immediate one-hop callers/consumers.

## Availability Gate

Run before Phase 1 (Guidelines).

- **PR Mode**: `ghHistoryResearch` responds and the PR is accessible (`prNumber` + `content: { metadata: true }` resolves). No fallback — if it fails, stop and ask for the correct PR number/URL/owner-repo.
- **Local Mode**: local tools respond (`localViewStructure` on the workspace root) and `git status` succeeds.
  `ENABLE_LOCAL` defaults to `true`.
  If a local tool fails, check `ENABLE_LOCAL` and `.octocoderc` (`docs/CONFIGURATION.md`) before declaring local tools disabled.
  If disabled and unavailable, offer to review a pushed PR instead.
  `lspGetSemantics` failure alone is not a hard stop; fall back to `localSearchCode` pattern matching.
- **Local Mode**: confirm at least one of staged, unstaged, or untracked changes exists; if none, stop and tell the user to stage/modify files first, or check they're on the intended branch.
- **File Scope**: the requested file exists (see Target Detection).

## Review Mode Sizing

| Mode | Trigger | Behavior |
|---|---|---|
| Quick | ≤5 files changed AND all LOW risk (docs/style/config-only) | Surface scan only — skip the deep Analysis pass; go Checkpoint -> Finalize. |
| Full | >5 files OR any HIGH/MED risk file OR user requests full review | Run every step below. |

Default to Full when uncertain. User's explicit choice overrides the trigger either direction.

## Rule Precedence

When guidance conflicts, higher priority wins; document the conflict in the review rather than silently picking one.

| Priority | Source |
|---|---|
| 1 (highest) | User-provided guidelines (file path or inline text given in the Guidelines Gate) |
| 2 | `.octocode/pr-guidelines.md`, if present |
| 3 | `.octocode/context/context.md`, `CONTRIBUTING.md`, `AGENTS.md` |
| 4 | Domain reviewer defaults (below) |
| 5 (lowest) | Soft preferences — style, readability |

## Guidelines Gate

1. Check for existing context files.
   Local Mode, or PR Mode inside the PR repo: use `localFindFiles`.
   Search for `.octocode/pr-guidelines.md`, `.octocode/context/context.md`, `CONTRIBUTING.md`, and `AGENTS.md`.
   PR Mode outside the PR repo: use `ghSearchCode` for `pr-guidelines`, `CONTRIBUTING`, and `AGENTS`.
   Read hits with `localGetFileContent`/`ghGetFileContent` and tell the user what was found.
2. Ask the user: "Do you have any guidelines files or context documents I should use for this review? A file path, inline text, or 'skip' is fine."
3. Read any path the user gives via `localGetFileContent`/`ghGetFileContent`; store inline text as-is; if "skip", still use whatever auto-discovered files were found in step 1.
4. Carry the combined guidelines context (source, priority, rules) through Analysis, Finalize, and Report — flag violations there, don't just collect and drop them.

## Context Collection

**PR Mode**:
1. Call `ghHistoryResearch({ prNumber, content: { metadata: true, changedFiles: true } })`.
2. Fetch existing review comments with `content: { comments: { reviewInline: true, discussion: true } }`.
3. Note which comments are fixed versus still open; do not duplicate open comments as new findings.
4. Fetch commits with `content: { commits: true }` for development progression.
5. Fetch selected patches for high-risk files with `content: { patches: { mode: "selected", files: [...] } }`.
6. Use `mode: "all"` only for small PRs or when asked; it can return 100k+ characters.
7. Past roughly 2000 changed lines, stay on selected patches and process high-risk files first.
8. `reviewMode: "full"` is acceptable for small PRs.

**Local Mode**:
1. Run `git status` for staged, unstaged, and untracked files.
2. Run `git diff --staged` and/or `git diff` for the user's stated scope.
3. Use `git diff HEAD` only when the user wants staged and unstaged changes combined.
4. Run `git log --oneline -10` and `git branch --show-current` for context.
5. Read each changed symbol with `localGetFileContent(matchString: <changed symbol>)`.
6. Use `localViewStructure` on parent directories for changed files.
7. If the diff is too large, ask for scope instead of swallowing everything.

**Both modes**:
- Classify risk per file: HIGH (auth/data/API/logic changes) vs LOW (docs/CSS/config-only).
- Health check: flag oversized diffs (>500 lines) for splitting; flag unrelated areas mixed in one PR/commit for splitting; PR Mode also flags a missing description or missing ticket/issue reference.
- Group changed files by functional area (e.g., "Auth: src/auth/login.ts, src/auth/middleware.ts") — this grouping drives the Checkpoint summary and any parallel-agent split.

## User Checkpoint

Present a TL;DR before deep analysis: overview, files/areas, risk, sizing, guidelines, and early concerns.
For Local Mode, split staged, unstaged, and untracked files/lines.
Ask: "Which areas should I focus on, or should I do a full review?"
Wait for the response unless the change is tiny, unambiguous, and all LOW risk.
Apply focus/context to Analysis; if they want only a summary, skip to Finalize/Report with current findings.
If unresponsive, wait.

## Tool Selection Rules

| Review target | Primary | Secondary | Avoid |
|---|---|---|---|
| PR Mode, workspace IS the PR's repo | `local*` + `lspGetSemantics` | `ghHistoryResearch` for PR metadata/diff/comments | shell for code reading |
| PR Mode, workspace is NOT the PR's repo | `ghSearchCode`/`ghGetFileContent`/`ghViewRepoStructure` | `npmSearch` for external deps | `local*`/`lspGetSemantics` (wrong repo) |
| Local Mode | `local*` + `lspGetSemantics` + shell `git` | `npmSearch` for external deps | `gh*` (not needed) |

`git` shell commands are only for status, diff, log, and branch.
All code reading and search goes through Octocode tools.
In Full mode, track progress with your runtime's task/todo tool.
Use: target detected -> gate passed -> guidelines gathered -> context collected -> checkpoint -> analysis -> finalized -> reported.
Quick mode and File Scope do not need progress tracking.

Tool transitions:

| From | Need | Go to |
|---|---|---|
| `ghSearchCode` | file content | `ghGetFileContent` |
| `ghSearchCode` | package source | `npmSearch` -> `ghViewRepoStructure` |
| `ghHistoryResearch` | file content on a changed path | `ghGetFileContent` |
| `import` statement | external definition | `npmSearch` -> `ghViewRepoStructure` |
| `localSearchCode` | definition/references/callers/callees | `lspGetSemantics(type: ..., lineHint: <from search>)` |
| `git diff` output | deep analysis of changed code | `localSearchCode` -> `lspGetSemantics` |
| `git status` output | read a changed file | `localGetFileContent(matchString: ...)` |

`localSearchCode` (or a structural match) is always the step before `lspGetSemantics` — it produces the `lineHint` every symbol-anchored LSP `type` requires. Never guess `lineHint`.

## Flow Analysis Recipes

Match the recipe to what actually changed; run it on every changed public/high-risk symbol.

| Changed code | Recipe | Local/PR-in-repo | Remote-only |
|---|---|---|---|
| Function signature changed | incoming callers | `localSearchCode` for `lineHint` -> `lspGetSemantics(type: "callers", symbolName, lineHint)` | `ghSearchCode` for the symbol name -> `ghGetFileContent(matchString: ...)` on each hit |
| New function added | outgoing dependencies | `lspGetSemantics(type: "callees", symbolName, lineHint)` | — |
| Type/interface changed | all usages | `lspGetSemantics(type: "references", symbolName, lineHint, includeDeclaration: true)` | `ghSearchCode` for the type name |
| Data transformation changed | trace the chain | chain `lspGetSemantics(type: "callees")` hop by hop; exact-read each boundary with `localGetFileContent` | — |
| Export removed/renamed | import chain | `lspGetSemantics(type: "references")` locally | `ghSearchCode({ keywords: ["import", "<name>"] })` for remote consumers |

For every traced symbol, document changed return values, changed types, side effects, integration breaks, and blast radius.
Do not stop at the hit list.
For each caller, consumer, or dependency, read it to confirm whether it actually breaks.
Continue with `workflow-pr-review-analysis.md` for reviewers, flow analysis, multi-agent split, and finding shape.
Use `workflow-pr-review-report.md` for report template, delivery rules, and checklist.

Validate: `node scripts/eval-research.mjs --case pr-local-review`.
