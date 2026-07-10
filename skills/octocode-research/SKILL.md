---
name: octocode-research
description: "Use when code needs evidence-first research: find where X lives, how it works, debug a failure, change after proof, PR/local-diff or safe-to-merge review, GitHub/npm prior art, dead code, architecture, or ReAct loops."
---

# Octocode Research

Evidence-first technical research and code work: `SCOPE -> SEARCH -> READ EXACT -> VALIDATE -> DECIDE/PATCH -> VERIFY`.
Octocode surfaces: LOCAL · EXTERNAL · FEDERATED. Check `context`, `auth status`, `lsp-server status <file>` before trusting a surface.

## Modes
Investigate; Review PRs/diffs (`references/workflow-pr-review.md`); Change after evidence; Map/Validate prior art; Loop when evidence shifts.

## Rules
1. State corpus, question, mode, and active/skipped surfaces in one line.
2. Route by what you hold; never force grep → AST → LSP.
3. Nontrivial code claims: read ≥2 of structure, stream, connections.
4. Ledger: `claim -> evidence -> confidence -> next check`.
5. Ask before broad public contracts, deletes/renames, thin evidence, or 3+ unrelated spaces.
6. Edits: smallest scoped patch; report checks that **actually ran**.
7. Broad/contested work: fan out subagents, merge on conflict, re-verify anchors.
8. **Results beat words:** cite `file:line` / exact reads and report exit codes from checks that actually ran.

## Reference Map
Before any task, read `references/algorithm.md` first (routing, evidence grades, failure signals).
- when planning/measuring/fan-out: `references/researcher-mindset.md` — progress and subagent fan-out
- when choosing a mode route: `references/workflows.md` — index to local/external/debug/PR/change
- when investigating code: `references/code-research.md` — code investigation and change paths
- when general research/validation: `references/research-flow.md` — non-code research flow
- when ranking repos: `references/github-landscape.md` — ecosystem prior art
- when contested/deep decisions: `references/long-research.md` — durable decision brief
- when evidence keeps shifting: `references/loop-mode.md` — Act→Observe→Learn until evidence converges
- when improving with goal→KPI: prefer `octocode-eval`; else stub `references/improve-loop.md`
- when command/MCP/schema details matter: `references/octocode.md` — transport and tool syntax
Should-we-build → `octocode-brainstorming`.

## Related skills
- `octocode-eval` — goal→KPI / held-out (not investigation loops)
- `octocode-brainstorming` — worth-building · `octocode-rfc-generator` — decision docs
- `octocode-roast` — critique · `octocode-subagent` — fan-out · `octocode-awareness` — locks/memory
- `octocode-skills` — edit/review this skill folder

## Scripts
- `scripts/eval-research.mjs` — when changing this skill; run the matching eval case.

## Output
Quick: `Finding`, `Evidence`, `Confidence`, `Next`.
Decision/review: `TL;DR`, evidence, verdict, risks, `file:line`, verification, confidence, smallest safe fix.
