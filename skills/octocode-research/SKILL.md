---
name: octocode-research
description: "Use when technical or code work needs evidence-first research: investigate, implement, review, refactor, map prior art, run iterative research loops (Act→Observe→Learn), validate findings, inspect artifacts, or plan with citations before acting."
---

# Octocode Research

Lead evidence-first technical research and code work. Flow: `SCOPE -> SEARCH -> READ EXACT -> VALIDATE -> DECIDE/PATCH -> VERIFY`.

## Modes

Map/Validate for landscape, prior art, or whether to add/build something. Investigate/Review for root cause, behavior, provenance, PR/local diff findings. Plan/Change for implementation, refactor, architecture, migration; edit only when asked. Loop when one pass is insufficient.

## Operating Rules

- State corpus, question, mode, and active/skipped surfaces in one line.
- Use MCP tools when exposed; otherwise use `npx octocode` after reading schemas/help.
- Start cheap with tree/path/package/repo discovery; deep-read exact slices only after anchors appear.
- Cross-pollinate surfaces: local clues feed GitHub/npm/web, and external claims feed code reads.
- Keep a claim ledger; promote snippets to proof only after exact source, AST/LSP, history, artifact, or test evidence.
- Recall prior lessons first; record durable findings only when a reusable lesson survives rebuttal.
- Ask before broad public-contract changes, materially conflicting evidence, thin surfaces after retries, or 3+ unrelated problem spaces.
- For code edits, make the smallest scoped patch and report actual verification.

## Reference Map

- `references/octocode.md` — when choosing transport, auth, install, schema, or CLI/MCP fallback behavior.
- `references/research-flow.md` — when running Map, Validate, Investigate, prior-art, PR/history, package, or multi-surface research.
- `references/loop-mode.md` — when repeating Act→Observe→Learn cycles until evidence converges.
- `references/code-research.md` — when doing implementation, review, refactor, architecture, dead-code, binary, or blast-radius work.
- `references/finding-checks.md` — when validating, dismissing, or presenting findings before a report or patch.
- `references/long-research.md` — when writing a durable decision brief, saved artifacts, or audit trail.
- `references/github-landscape.md` — when comparing GitHub repos, packages, reuse options, or ecosystem candidates.

## Route Quick Pick

Map/Validate → `research-flow`; Investigate/Review → `research-flow` + `code-research` + `finding-checks`; Plan/Change → `code-research`; Loop → `loop-mode`; Long/contested → `long-research`; ecosystem comparison → `github-landscape`.

## Scripts

- `scripts/eval-research.mjs` — self-test and evaluate research answers when changing this skill.

## Output

Quick answer: `Finding`, `Evidence`, `Confidence`, `Next`. Decision brief: `TL;DR`, `scope`, `evidence by surface`, `verdict`, `risks/gaps`, `next step`. Review/code output: severity-ranked `file:line` findings, verification, confidence, and smallest safe fix.
