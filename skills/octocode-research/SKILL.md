---
name: octocode-research
description: "Use when technical or code work needs evidence-first research: investigate, implement, review, refactor, map prior art, run iterative research loops (Act→Observe→Learn), validate findings, inspect artifacts, or plan with citations before acting."
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

- `references/algorithm.md` — read at session start or before nontrivial work for router, evidence grades, and failure signals.
- `references/research-flow.md` — when mapping, validating, investigating, planning, or composing multi-surface flows.
- `references/workflows.md` — when choosing local, external, root-cause, or PR/local-review workflow shape.
- `references/code-research.md` — before code investigation, review, refactor, architecture, dead-code, binary, or blast-radius claims.
- `references/loop-mode.md` — when one pass is insufficient and Act→Observe→Learn cycles need convergence.
- `references/octocode.md` — when choosing MCP vs CLI transport, auth, install, or command syntax.
- `references/long-research.md` — when a durable brief, artifact, claim ledger, or audit trail is needed.
- `references/github-landscape.md` — when comparing GitHub repos, package ecosystems, or reuse candidates.

## Scripts

- `scripts/eval-research.mjs` — self-test and evaluate research answers when changing this skill.

## Output

Quick answer: `Finding`, `Evidence`, `Confidence`, `Next`. Decision brief: `TL;DR`, `scope`, `evidence by surface`, `verdict`, `risks/gaps`, `next step`. Review/code output: severity-ranked `file:line` findings, verification, confidence, and smallest safe fix.
