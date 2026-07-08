# Workflow: PR Or Local Review

Use for `review PR`, `safe to merge`, `review my changes`, staged/unstaged diffs, and local file-scope review. Read `algorithm.md` first for the router and evidence grades; use `code-research.md` for the proof ladder on any finding.
Review changed code and direct blast radius; avoid style-only, unchanged-code, generated/vendor, and already-commented findings unless unresolved.

This file adapts every instruction from the standalone [`octocode-pull-request-reviewer`](https://github.com/bgauryy/octocode/blob/benchmark/skills/octocode-pull-request-reviewer/SKILL.md) skill to Octocode's actual tools and this repo's verified config (`docs/OCTOCODE_TOOLS.md`, `docs/CONFIGURATION.md`). That source skill names tools that do not exist here (`githubSearchPullRequests`, `packageSearch`, `lspGotoDefinition`/`lspFindReferences`/`lspCallHierarchy`, `githubViewRepoStructure`) — every mention below uses the real equivalents: `ghHistoryResearch`, `npmSearch`, `lspGetSemantics(type: ...)`, `ghViewRepoStructure`.

## Target Detection

Run first, before any tool call.

| User input | Target | Mode |
|---|---|---|
| PR number ("review PR #123") or PR URL | Remote PR | PR Mode |
| Branch name with PR context | Remote PR | PR Mode |
| Specific file path, no PR context | Local file | Local Mode (File Scope) |
| "review my changes/diff", "review staged/unstaged changes", "review local changes" | Local changes | Local Mode |
| No PR specified, "review my code" | Local changes | Local Mode |
| Ambiguous | — | Ask: "Would you like me to review a specific PR or your local changes?" |

File Scope: verify the file exists (`localFindFiles`/`localViewStructure`) before anything else; if missing, stop and ask for the correct path. If it exists, scope analysis to the file plus its direct imports/exports and immediate (1-hop) callers/consumers — do not expand to a full-repo review unless asked.

## Availability Gate

Run before Phase 1 (Guidelines).

- **PR Mode**: `ghHistoryResearch` responds and the PR is accessible (`prNumber` + `content: { metadata: true }` resolves). No fallback — if it fails, stop and ask for the correct PR number/URL/owner-repo.
- **Local Mode**: local tools respond (`localViewStructure` on the workspace root) and `git status` succeeds. `ENABLE_LOCAL` defaults to `true`; if a local tool fails, check `ENABLE_LOCAL` isn't `false` and `local.enabled` isn't `false` in `.octocoderc` (`docs/CONFIGURATION.md`) before concluding local tools are disabled. If disabled and can't be enabled, offer to review a pushed PR instead. `lspGetSemantics` failure alone is not a hard stop — fall back to `localSearchCode` pattern matching for that step.
- **Local Mode**: confirm at least one of staged, unstaged, or untracked changes exists; if none, stop and tell the user to stage/modify files first.
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

Before collecting the diff:

1. Check for existing context files. Local Mode / PR Mode where the workspace is the PR's repo: `localFindFiles` for `.octocode/pr-guidelines.md`, `.octocode/context/context.md`, `CONTRIBUTING.md`, `AGENTS.md`. PR Mode where the workspace is not the PR's repo: `ghSearchCode({ keywords: ["pr-guidelines","CONTRIBUTING","AGENTS"], match: "path", owner, repo })`. Read any hits with `localGetFileContent`/`ghGetFileContent` and tell the user what was found.
2. Ask the user: "Do you have any guidelines files or context documents I should use for this review? A file path, inline text, or 'skip' is fine."
3. Read any path the user gives via `localGetFileContent`/`ghGetFileContent`; store inline text as-is; if "skip", still use whatever auto-discovered files were found in step 1.
4. Carry the combined guidelines context (source, priority, rules) through Analysis, Finalize, and Report — flag violations there, don't just collect and drop them.

## Context Collection

**PR Mode**: `ghHistoryResearch({ prNumber, content: { metadata: true, changedFiles: true } })` first, then `content: { comments: { reviewInline: true, discussion: true } }` for existing review comments (note which were fixed vs still open — don't duplicate open ones as new findings), then `content: { commits: true }` for development progression, then `content: { patches: { mode: "selected", files: [...] } }` scoped to high-risk files. Use `content: { patches: { mode: "all" } }` only for small PRs or when asked — it can return 100k+ chars unbounded. `reviewMode: "full"` is a shortcut for all of the above in one call when the PR is small.

**Local Mode**: `git status` for staged/unstaged/untracked; `git diff --staged` and/or `git diff` per the user's stated scope (respect "staged only"/"unstaged only"); `git diff HEAD` when both exist and the user wants a combined view; `git log --oneline -10` and `git branch --show-current` for context. Then `localGetFileContent(matchString: <changed symbol>)` and `localViewStructure` on parent directories for each changed file.

**Both modes**:
- Classify risk per file: HIGH (auth/data/API/logic changes) vs LOW (docs/CSS/config-only).
- Health check: flag oversized diffs (>500 lines) for splitting; flag unrelated areas mixed in one PR/commit for splitting; PR Mode also flags a missing description or missing ticket/issue reference.
- Group changed files by functional area (e.g., "Auth: src/auth/login.ts, src/auth/middleware.ts") — this grouping drives the Checkpoint summary and any parallel-agent split.

## User Checkpoint

Stop here and present a TL;DR before deep analysis: overview (1-2 sentences), files/areas changed, risk assessment with reasoning, Quick/Full sizing with reasoning, guidelines loaded (or "none"), and any early concerns. Then ask: "Which areas should I focus on, or should I do a full review?" Wait for the response before continuing — proceed with stated assumptions only for a tiny, unambiguous, all-LOW-risk change. If the user says "just give me the summary," skip straight to Finalize/Report with whatever was already found.

## Tool Selection Rules

| Review target | Primary | Secondary | Avoid |
|---|---|---|---|
| PR Mode, workspace IS the PR's repo | `local*` + `lspGetSemantics` | `ghHistoryResearch` for PR metadata/diff/comments | shell for code reading |
| PR Mode, workspace is NOT the PR's repo | `ghSearchCode`/`ghGetFileContent`/`ghViewRepoStructure` | `npmSearch` for external deps | `local*`/`lspGetSemantics` (wrong repo) |
| Local Mode | `local*` + `lspGetSemantics` + shell `git` | `npmSearch` for external deps | `gh*` (not needed) |

`git` shell commands are only for status/diff/log/branch — all code reading and search goes through Octocode tools, never `cat`/`grep`/`find`/`curl`/`gh` CLI.

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

For every traced symbol, document: return values/types/side effects that changed, whether existing integrations break, and the blast radius (how many callers/consumers are affected).

## Domain Reviewers

| Domain | Detect | HIGH priority | MED priority | Skip |
|---|---|---|---|---|
| Bug | Runtime errors, logic flaws, data corruption, resource leaks, race conditions, type violations, API misuse | Crashes, data corruption, security breach, null access in a hot path | Edge-case errors, uncertain race conditions | Try/catch without cleanup need, compiler-caught issues |
| Architecture | Pattern violations, tight coupling, circular deps, mixed concerns, leaky abstractions | Breaking public API, circular deps causing bugs | Significant pattern deviations, tech-debt increase | Single-file organization, framework-standard patterns |
| Performance | O(n^2) where O(n) is possible, blocking ops, missing cache, unbatched ops, memory leaks | O(n^2) on large datasets, memory leaks, blocking the main thread | Moderate inefficiency in frequent paths | Negligible impact, theoretical improvements |
| Code Quality | Naming violations, convention breaks, visible typos, magic numbers, TODO in new code | Typos in a public API/endpoint | Internal naming issues, DRY violations, convention deviations | Personal style, linter-handled formatting |
| Duplicate Code | Missed opportunities to reuse existing code/utilities/patterns | Missing use of a critical utility that could prevent bugs | Duplication violating DRY across files | Intentional duplication for clarity |
| Error Handling | Poor error messages, unclear logs, swallowed exceptions, missing debug context | Swallowed exceptions hiding critical failures | Unclear error messages, missing log context | Internal service calls in trusted environments |
| Flow Impact | How changes alter execution flow, data paths, system behavior — trace with the Flow Analysis Recipes above | Changes that break callers, alter critical paths, change data-flow semantics | Flow changes requiring dependent-code updates, altered return values/types | Internal refactors with unchanged external behavior |

Global exclusions — never suggest: compiler/linter errors (tooling already catches these), unchanged code (no `+` prefix), test implementation details unless broken, generated/vendor files, speculative "what if" scenarios, issues already raised in existing PR comments.

## Review Confidence Model

Keep two axes distinct — do not conflate them:

- **Severity** (impact if true): `HIGH`/`MED`/`LOW`, per the domain table above.
- **Confidence** (how sure the evidence makes you): `confirmed`/`likely`/`uncertain`, per `algorithm.md`'s evidence grades — same vocabulary as every other workflow in this skill.

A HIGH-confidence typo is still LOW severity; a `likely`-confidence security flaw still gets flagged, just marked uncertain rather than confirmed. Include a finding when it is `confirmed` or `likely` AND touches new/changed code (`+` lines) AND is a real, actionable problem. Skip or explicitly mark `uncertain` findings that need more evidence rather than asserting them.

Mindset: focus on changed code only — added lines (`+`), modified lines (new implementation, but consider removed context), and deleted code only when the removal creates a new risk. Think like a parser: trace `import {X} from 'Y'` to `Y`'s definition, follow entry -> propagation -> termination, and use `localSearchCode`/AST structural matches (not guesswork) to confirm the shape before calling it a finding.

## Analysis

Run for every user-specified focus area (or all domains in Full mode without a stated focus):

1. List 3-5 search queries aligned with the focus, execute each, and name the goal per query.
2. Guidelines compliance: check each changed file against the loaded guidelines context; flag violations with a specific reference: `[GUIDELINE: <source> — <rule>]`.
3. Flow impact analysis (required for every function/method change): apply the matching Flow Analysis Recipe; document blast radius.
4. Validate schemas/APIs/dependencies with `matchString`-targeted reads (`ghGetFileContent`/`localGetFileContent` + `localSearchCode`).
5. Assess impact per angle, prioritizing the user's stated focus: architectural (structure, pattern alignment), integration (affected systems/patterns), risk (race conditions, performance, security), business (UX, metrics, operational cost), cascade (could this cause other problems downstream).
6. Identify edge cases in the changed logic.
7. Security scan: injection, XSS, data exposure, auth bypass, hardcoded secrets, regulatory-compliance patterns where relevant.
8. Scan new code (`+` lines only) for TODO/FIXME.
9. For high-risk changes, assess whether a rollback strategy or feature flag is needed.
10. Local Mode only: if changes are substantial, suggest running the project's test/lint suite before finalizing.

Do not analyze areas the user explicitly excluded at Checkpoint. Do not use `gh*` tools for code reading in Local Mode, or `local*`/`lsp*` in PR Mode when the workspace isn't the PR's repo. If a search returns nothing, broaden the query or change tool before concluding absence; if flow tracing dead-ends, document the limitation and proceed with available evidence.

## Multi-Agent Parallelization (only if your runtime supports spawning subagents)

Applies to both PR and Local Mode; skip entirely in Quick mode or file-scope review (single-pass only).

| Files changed | Agents |
|---|---|
| ≤5 (Quick) | none — single-pass |
| 6-15 | 2: Flow Impact + Architecture/Quality |
| 16-30 | 3: add Security & Error Handling |
| 30+ | 4: add Guidelines & Duplicates (only if guidelines were loaded) |

Spawn all agents for a batch in a single message — sequential spawning defeats the purpose. Each agent uses the same mode-appropriate tools as above (no `gh*` for code reading in Local Mode agents).

- **Flow Impact**: for every modified symbol, `localSearchCode` -> `lspGetSemantics(type: "callers"/"references")` (or `ghSearchCode`/`ghGetFileContent` remotely) -> document `{symbol, file:line, callers, breaking: bool}`.
- **Security & Error Handling**: scan changed files for injection/XSS/data exposure/auth bypass/hardcoded secrets and swallowed exceptions/missing error context; only flag `+` lines.
- **Architecture & Code Quality**: compare changed code against existing repo patterns (`ghViewRepoStructure`/`localViewStructure` for layout, `localSearchCode` for existing patterns); flag coupling, naming, performance smells, TODO/FIXME in new code.
- **Guidelines & Duplicates** (only if guidelines loaded): check each changed file against every loaded rule; search for existing utilities the new code should have reused instead of duplicating.

Merge (orchestrator): collect all findings, dedupe by root cause or same `file:line` (keep the higher-confidence one; merge cross-domain hits into one finding listing both domains), cross-check against existing PR comments, prioritize Security > Bug > Flow Impact > Architecture > Performance > Quality > Duplicates, cap to the top ~5-7. Do not proceed to Finalize before every spawned agent has returned; agents don't write files or modify code directly.

## Finalize

1. Dedupe against existing PR comments (PR Mode) — merge findings sharing a root cause with an already-open comment; don't restate them as new.
2. Refine every MED/LOW-confidence finding with one more targeted search: `UNCHANGED` (verified correct), `UPDATED` (new context improved it), or `INCORRECT` (delete it).
3. Verify against loaded guidelines: flag violations as `[GUIDELINE: <source> — <rule>]`; if a finding contradicts a guideline, the guideline wins per the Rule Precedence table — document the conflict.
4. Every surviving finding needs: `confirmed`/`likely` confidence, exact `file:line`, and an actionable fix (diff format). PR Mode also needs open-comment resolution checked (re-flag as unresolved if a prior comment's issue wasn't actually fixed).
5. Cap to the ~5-7 most impactful findings, prioritized HIGH severity first, then by domain weight from the Multi-Agent merge order above. Move lower-priority items to an "additional notes" section rather than dropping them silently.

## Finding Shape

Every surviving finding uses this shape, whether shown in chat or written to a document:

```text
[DOMAIN-1] title
Severity: HIGH|MED|LOW
Confidence: confirmed|likely|uncertain
Location: path:line
Evidence: exact read + proof lane
Impact: caller/user/data/contract consequence
Fix: minimal code direction or diff
```

Use `[SEC-1]`/`[BUG-1]`-style descriptive IDs, plain `1.`/`2.`, or lettered labels — never `#1`/`#2`/`#N` (GitHub auto-links `#<number>` to issues/PRs).

## Report

1. Present a chat summary first, before writing any file: recommendation (`APPROVE`/`REQUEST_CHANGES`/`COMMENT` for PR Mode, `LOOKS_GOOD`/`NEEDS_CHANGES`/`COMMENT` for Local Mode), risk level, findings grouped High/Medium/Low with `file:line`, and guidelines status (violations / all pass / none loaded).
2. Ask before creating a document: "Would you like me to create the detailed review document?" Only write a file after the user says yes.
3. If approved, write to `.octocode/reviewPR/<session-name>/PR_<prNumber>.md` (PR Mode) or `.octocode/reviewLocal/<session-name>/REVIEW_<branch>_<timestamp>.md` (Local Mode) — `<session-name>` is a short descriptive slug (e.g. `auth-refactor`). If the write fails, output the document content in chat instead.

Output template (both modes): Executive Summary (goal/scope, files/lines changed, risk + reasoning, review mode, recommendation, affected areas, flow changes) -> Ratings (correctness/security/performance/maintainability, X/5) -> PR/Changes Health (description, ticket reference, size, tests — PR Mode; cohesion, size, tests — Local Mode) -> Guidelines Compliance table (source/rule/status) if guidelines were loaded -> Issues by priority, each as: title, `Location: path:line`, `Confidence: confirmed|likely|uncertain`, `Severity: HIGH|MED|LOW`, 1-2 sentence explanation, `Fix` as a diff block -> Flow Impact Analysis (affected callers/consumers, or a diagram) when changes are significant -> (Local Mode) Suggested Next Steps (run tests / fix issues / split into commits / ready to commit).

Tone: professional, constructive, about the code not the author; explain reasoning; distinguish requirements from preferences. Use full `https://github.com/{owner}/{repo}/blob/{branch}/{path}` links for PR Mode code references; use `file:line` for Local Mode. Never give timing/duration estimates.

## Verification Checklist

Before delivering the review, confirm:

- [ ] Target/mode resolved (including file-scoped local checks when requested).
- [ ] Availability Gate passed for the resolved mode.
- [ ] User was asked for guidelines/context files (Guidelines Gate ran, even if the answer was "skip").
- [ ] Diff/PR context collected per mode (metadata + changedFiles + comments + commits for PR Mode; `git status`/`diff`/`log`/`branch` for Local Mode).
- [ ] User Checkpoint presented and a response received (or explicitly a tiny/low-risk exception).
- [ ] Flow impact analyzed for every modified function/method (Full mode).
- [ ] All user-specified focus areas covered; no excluded areas analyzed.
- [ ] Findings deduped against existing PR comments (PR Mode) and against each other.
- [ ] Every finding has `file:line`, severity, confidence, evidence, and a code fix.
- [ ] Guidelines compliance checked and reported if guidelines were loaded.
- [ ] No `#<number>` notation used anywhere in the output.
- [ ] Chat summary presented and user asked before any document was written.

Validate: `node scripts/eval-research.mjs --case pr-local-review`.
