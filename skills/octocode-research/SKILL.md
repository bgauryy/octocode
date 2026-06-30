---
name: octocode-research
description: "Use when technical or code work needs evidence-first research: investigate, implement, review, refactor, map prior art, run Act→Observe→Learn loops, validate findings, inspect artifacts, or plan with citations before acting."
---

# Octocode Research

Lead evidence-first technical research and code work: investigate, plan, review, change, validate, map prior art, inspect artifacts, and loop until evidence converges. Flow: `SCOPE -> SEARCH -> READ EXACT -> VALIDATE -> DECIDE/PATCH -> VERIFY`.

## Modes

- Map: landscape or "what exists" questions.
- Validate: "is this worth it" or "should we add X" decisions.
- Investigate: root cause, behavior, provenance, or code explanation.
- Plan: implementation, refactor, architecture, migration, or blast-radius planning.
- Review: PR/local diff findings ordered by severity.
- Change: user clearly asked for code edits now.
- Loop: one pass is insufficient or the user asks to iterate until proof.
Default to Investigate for concrete behavior, Validate for ambiguous research, Plan before risky edits, and Change only when edits are requested.

## Operating Rules

- State corpus, question, mode, and active/skipped surfaces in one line.
- Use MCP tools when exposed; otherwise use `npx octocode`; read schemas/help before raw calls.
- Start cheap with tree/path/package/repo discovery; deep-read exact slices only after anchors appear.
- Cross-pollinate surfaces: local clues feed GitHub/npm/web, and external claims feed code reads.
- Keep a claim ledger; promote snippets to proof only after exact source, AST/LSP, history, artifact, or test evidence. Before researching, recall prior lessons with octocode-awareness (`get-memory --smart --query <question>`); on zero results retry synonyms/source locators and validate recalled code facts. When a durable finding converges, emit one awareness capture packet (or a `doNotCaptureReason`) using `learning-capture.md` so it is not re-researched later.
- Ask before broad public-contract changes, materially conflicting evidence, thin surfaces after retries, or 3+ unrelated problem spaces.
- For code edits, make the smallest scoped patch and report actual verification.

## Reference Map

- `references/octocode.md` — when choosing transport, auth, install, schema, or CLI/MCP fallback behavior.
- `references/research-flow.md` — when running Map, Validate, prior-art, PR/history, package, or multi-surface research.
- `references/code-research.md` — when implementation, review, refactor, architecture, dead-code, binary, or blast-radius work is likely.
- `references/loop-research.md` — when repeated Act->Observe->Learn loops, convergence proof, or no-progress handling matter.
- `references/finding-checks.md` — when validating, dismissing, or presenting findings before a report or patch.
- `references/long-research.md` — when the task needs a durable decision brief, saved artifacts, or audit trail.
- `references/github-landscape.md` — when comparing GitHub repos, packages, reuse options, or ecosystem candidates.

## Scripts

- `scripts/eval-research.mjs` — self-test and evaluate research answers against prompts when changing this skill.

## Output

Quick answer: `Finding`, `Evidence`, `Confidence`, `Next`. Decision brief: `TL;DR`, `scope`, `evidence by surface`, `what survived rebuttal`, `verdict`, `risks/gaps`, `next step`. Review/code output: severity-ranked `file:line` findings, verification, confidence, and smallest safe fix.

Install hint: `npx octocode skill --name octocode-research`.
