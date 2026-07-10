---
name: octocode-research
description: "Use when code work needs evidence-first research, debugging, change, PR/local-diff or safe-to-merge review, GitHub/npm prior art, dead code, architecture, or ReAct loops. Sole Octocode research and review skill."
---

# Octocode Research

Evidence-first technical research and code work: `SCOPE -> SEARCH -> READ EXACT -> VALIDATE -> DECIDE/PATCH -> VERIFY`.

Octocode spans three surfaces — LOCAL (workspace, `node_modules`, clones), EXTERNAL (GitHub, npm), FEDERATED (one OQL query across both). Check the environment before trusting a surface: `context`, `auth status`, `lsp-server status <file>`.

## Modes

Investigate behavior/root cause; Review PRs (URL/#N) and local/staged diffs via `workflow-pr-review.md`; Change after evidence; Map/Validate prior art or architecture; Loop when evidence shifts.

## Rules

1. State corpus, question, mode, and active/skipped surfaces in one line.
2. Route by what you already hold; never force a fixed grep -> AST -> LSP path.
3. For nontrivial code claims, read at least two of structure, stream, and connections.
4. Keep a tiny ledger: `claim -> evidence -> confidence -> next check`.
5. Ask before broad public contracts, deletes/renames, thin evidence, or 3+ unrelated problem spaces.
6. For code edits, make the smallest scoped patch and report the checks that actually ran.
7. For broad, contested, or multi-part questions, fan out parallel subagents on different directions or lanes, merge on conflict, and re-verify each worker's key anchor before trusting it.

## Reference Map

Before any task, read `references/algorithm.md` first; it owns routing, evidence grades, and failure signals.

- `references/researcher-mindset.md` — for state of mind, planning/managing the task, how to measure progress, fanning out subagents across directions, and dot-to-dot efficiency.
- `references/workflows.md` — when choosing a mode-specific workflow index.
- `references/code-research.md` — when investigating, reviewing, refactoring, or changing code.
- `references/research-flow.md` — when doing general research or idea validation.
- `references/workflow-local.md` — when the local repo or installed dependency is source of truth.
- `references/workflow-external.md` — when using GitHub, npm, PRs, commits, or remote repos.
- `references/workflow-combination.md` — when a local clue points upstream or remote code needs local-grade (AST/LSP/negative) proof.
- `references/workflow-debug.md` — when debugging failures or proving root cause.
- `references/workflow-pr-review.md` — when reviewing PRs, diffs, or merge safety.
- `references/workflow-pr-review-analysis.md` — when PR review analysis and finding shape are needed.
- `references/workflow-pr-review-report.md` — when writing PR/local review summaries or documents.
- `references/workflow-change.md` — when implementing or refactoring after evidence.
- `references/github-landscape.md` — when ranking GitHub repos or ecosystem prior art.
- `references/long-research.md` — when a decision brief, audit trail, or contested question needs depth.
- `references/loop-mode.md` — when evidence keeps shifting or verification fails repeatedly.
- `references/octocode.md` — when command syntax, MCP transport, or raw tool schemas matter.

For divergent idea generation or "should we build X" validation, use the `octocode-brainstorming` skill instead of this one.

## Scripts

- `scripts/eval-research.mjs` — when changing this skill; run the matching eval case.

## Output

Quick answer: `Finding`, `Evidence`, `Confidence`, `Next`.
Decision/review: `TL;DR`, evidence, verdict, risks, `file:line`, verification, confidence, smallest safe fix.
