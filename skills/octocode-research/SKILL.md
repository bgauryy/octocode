---
name: octocode-research
description: "Use for evidence-first technical research and code work: research or understand code, investigate/deep-dive a bug or root cause, review a PR or local diff before merging, plan/implement/refactor with citations, map prior art or research external GitHub repos and packages, validate whether to build something, inspect artifacts/binaries, or run iterative research loops (Act→Observe→Learn) until evidence converges."
---

# Octocode Research

Evidence-first thinking algorithm for technical research and code work: `SCOPE -> SEARCH -> READ EXACT -> VALIDATE -> DECIDE/PATCH -> VERIFY`.

## Modes

| Mode | Use for |
|---|---|
| Map / Validate | landscape, prior art, whether to add/build something |
| Investigate / Review | root cause, behavior, provenance, PR/local diff findings |
| Plan / Change | implementation, refactor, architecture, migration — edit only when asked |
| Loop | one pass is insufficient — repeat Act→Observe→Learn until evidence converges |

## Operating Rules

1. State corpus, question, mode, and active/skipped surfaces in one line.
2. Route by what you already hold; never a fixed grep→AST→LSP pipeline.
3. Use MCP tools when exposed; otherwise `npx octocode` after reading schemas/help.
4. Start cheap: tree/path/package/repo discovery. Deep-read exact slices only after anchors appear.
5. For nontrivial code claims, read at least two of structure/stream/connections; triangulate claims with 2-3 batched angles (`algorithm.md`).
6. Keep a tiny ledger: `claim -> evidence -> confidence -> next check`; promote snippets only after exact source, AST/LSP, history, artifact, or test proof.
7. Cross-pollinate surfaces: local clues feed GitHub/npm/web; external claims feed code reads.
8. Recall prior lessons first; record durable findings only when a reusable lesson survives rebuttal.
9. Ask before broad public-contract changes, materially conflicting evidence, thin surfaces after retries, or 3+ unrelated problem spaces.
10. For code edits, make the smallest scoped patch and report the verification that actually ran.

## Reference Map

Always read `references/algorithm.md` first — router, evidence grades, and failure signals apply to every trigger below. `references/workflows.md` is the index for the `workflow-*.md` files; go straight to the named file once the trigger is clear.

| Trigger examples | Mode | Load |
|---|---|---|
| "research X", "understand this code", "how does X work", "explore this repo" | Investigate | `algorithm.md`, `research-flow.md` |
| local repo/checkout/installed dependency is the source of truth | Investigate | `algorithm.md`, `workflow-local.md` |
| remote repo, PR, package, or upstream dependency not present locally | Investigate | `algorithm.md`, `workflow-external.md` |
| "why does X fail", "root cause", "debug this", "deep dive into Y" | Investigate | `algorithm.md`, `workflow-debug.md`, `code-research.md` |
| "review PR #N", "review my changes/diff", "safe to merge", "review this file" | Review | `algorithm.md`, `workflow-pr-review.md`, `code-research.md` |
| "implement", "refactor", "migrate", "add X", "patch this" | Plan / Change | `algorithm.md`, `workflow-change.md`, `code-research.md` |
| "map prior art", "what exists for X", "landscape", "compare repos" | Map | `algorithm.md`, `research-flow.md`; add `github-landscape.md` for repo-ecosystem ranking |
| "should we build X", "validate this idea", "is this worth doing" | Validate | `algorithm.md`, `research-flow.md`; add `long-research.md` when long/contested |
| dead code, unused exports, reachability, drift | Investigate | `algorithm.md`, `code-research.md`, `research-flow.md` (OQL recipe) |
| "keep researching until sure", multi-pass convergence | Loop | `loop-mode.md` |
| MCP vs CLI, auth, install, command syntax | any | `octocode.md` |
| decision brief, claim ledger, audit trail | Validate / Map | `long-research.md` |

## Scripts

- `scripts/eval-research.mjs` — self-test and evaluate research answers when changing this skill; case definitions and pass criteria live in `evals/cases.json` and `evals/prompts.md`. Each `references/workflow-*.md` file names the eval case(s) that cover it — run the matching case after editing that workflow.

## Output

Quick answer: `Finding`, `Evidence`, `Confidence`, `Next`. Decision brief: `TL;DR`, `scope`, `evidence by surface`, `verdict`, `risks/gaps`, `next step`. Review/code output: severity-ranked `file:line` findings, verification, confidence, and smallest safe fix.
