---
name: octocode-research
description: "Use when technical or code work needs evidence-first research: investigate, implement, review, refactor, map prior art, run iterative research loops (Act→Observe→Learn), validate findings, inspect artifacts, or plan with citations before acting."
---

# Octocode Research

Lead evidence-first technical research and code work. Flow: `SCOPE -> SEARCH -> READ EXACT -> VALIDATE -> DECIDE/PATCH -> VERIFY`.

## Modes

Map/Validate for landscape, prior art, or whether to add/build something. Investigate/Review for root cause, behavior, provenance, PR/local diff findings. Plan/Change for implementation, refactor, architecture, migration; edit only when asked. Loop when one pass is insufficient.

## Operating Rules

1. State corpus, question, mode, and active/skipped surfaces in one line.
2. Route by what you already hold (`references/algorithm.md`) — never a fixed grep→AST→LSP pipeline.
3. Use MCP tools when exposed; otherwise `npx octocode` after reading schemas/help.
4. Start cheap: tree/path/package/repo discovery. Deep-read exact slices only after anchors appear.
5. Cross-pollinate surfaces: local clues feed GitHub/npm/web; external claims feed code reads.
6. Keep a claim ledger; promote a snippet to proof only after exact source, AST/LSP, history, artifact, or test evidence — never from a single evidence grade.
7. Recall prior lessons first; record durable findings only when a reusable lesson survives rebuttal.
8. Ask before broad public-contract changes, materially conflicting evidence, thin surfaces after retries, or 3+ unrelated problem spaces.
9. For code edits, make the smallest scoped patch and report actual verification.

## Reference Map

- `references/algorithm.md` — **read first, every task.** The router (route by what you hold), evidence grades, matchString-first, node_modules-first, anti-patterns, and failure signals. Every other reference assumes it.
- `references/octocode.md` — when choosing transport (MCP vs CLI), picking a tool, or needing auth/install/schema/CLI syntax. Full tool matrix.
- `references/research-flow.md` — when running Map, Validate, Investigate, Plan, prior-art, PR/history, package, or multi-surface research.
- `references/code-research.md` — when doing implementation, review, refactor, architecture, dead-code, binary, or blast-radius work, or before presenting/dismissing a finding (proof ladder + confidence rules included).
- `references/loop-mode.md` — when repeating Act→Observe→Learn cycles until evidence converges.
- `references/long-research.md` — rare path: durable decision brief, saved artifacts, or audit trail.
- `references/github-landscape.md` — rare path: comparing GitHub repos, packages, reuse options, or ecosystem candidates.

## Route Quick Pick

`algorithm.md` first, always. Map/Validate → `research-flow`; Investigate/Review → `research-flow` + `code-research`; Plan/Change → `code-research`; Loop → `loop-mode`; long/contested → `long-research`; ecosystem comparison → `github-landscape`.

## Scripts

- `scripts/eval-research.mjs` — self-test and evaluate research answers when changing this skill.

## Output

Quick answer: `Finding`, `Evidence`, `Confidence`, `Next`. Decision brief: `TL;DR`, `scope`, `evidence by surface`, `verdict`, `risks/gaps`, `next step`. Review/code output: severity-ranked `file:line` findings, verification, confidence, and smallest safe fix.
