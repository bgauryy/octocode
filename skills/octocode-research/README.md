# Octocode Research

`octocode-research` is the main evidence-first skill for technical and code work: code investigation, implementation planning, code changes, PR/local diff review, refactors, architecture analysis, dead-code proof, repeated Act -> Observe -> Learn loops, and finding validation.

It is the default when the answer or patch should be grounded in local code, GitHub, npm, PRs, history, artifacts, binaries, OQL packets, papers, specs, official docs, tests, AST, or LSP evidence.

Use it as the single technical-work skill: it covers both hands-on engineering tasks and iterative research loops without splitting those concerns into separate skill folders.

## How it works

The skill classifies the request as Map, Validate, Investigate, Plan, Review, Change, or Loop. It starts broad enough to avoid tunnel vision, deep-reads the strongest anchors, validates findings before presenting them, marks confidence explicitly, and either recommends one next action or applies the smallest verified code change the user asked for.

## Good asks

- "Research this technical area and tell me what matters."
- "Find the bug and fix it."
- "Review this PR or local diff."
- "Trace the blast radius before we rename this API."
- "Keep searching until you prove where this behavior comes from."
- "Find dead exports or safe-delete candidates."
- "What options exist across local code, GitHub, npm, and docs?"
- "Inspect this `.node`, `.wasm`, archive, or generated artifact."

## Features

- A one-line scope: question, corpus, mode, and active surfaces.
- A small hypothesis map before the agent narrows too early.
- Evidence grouped by surface rather than dumped as raw results.
- Exact anchors such as file:line, repo path, package id, PR number, commit, artifact fact, or fetched formal URL.
- Confidence marked as confirmed, likely, or uncertain.
- For loops: decisive observations, not a raw transcript.
- For code work: a patch, review report, investigation note, or safe next-step plan with verification.

## Modes

| Mode | Best for | User result |
|---|---|---|
| Map | Prior art or "what exists?" | Landscape clusters and the strongest evidence. |
| Validate | "Is this direction worth it?" | A verdict with supporting and weakening signals. |
| Investigate | "Why does this happen?" | Root cause or behavior explanation with proof. |
| Plan | "What path should we take?" | Current-state evidence, options, and a safe next step. |
| Review | PR/local diff or code-quality review | Severity-ranked findings with file:line citations. |
| Change | User asked for code changes now | Minimal patch plus actual verification. |
| Loop | Clear question needs convergence | Answer, evidence, loop trace, verification, and gaps. |

## Nearby skills

- The idea is still fuzzy or market-like: use `octocode-brainstorming`.
- The result should be a full RFC or proposal: use `octocode-rfc-generator`.
- The user wants harsh code-quality entertainment as the product: use `octocode-roast`.

## User value

This skill turns technical work into a disciplined evidence loop instead of a search-and-guess session. The user gets traceable proof, honest uncertainty, and the smallest safe action that follows from the evidence.

## For developers

Keep `SKILL.md` as a compact router for modes, evidence rules, references, scripts, and output shape. Put workflow depth in `references/research-flow.md`, code-change specifics in `references/code-research.md`, loop mechanics in `references/loop-research.md`, and long-report artifacts in `references/long-research.md`. Keep `references/octocode.md`, `references/github-landscape.md`, and `references/finding-checks.md` aligned when transport, external-research, or validation behavior changes. Use `scripts/eval-research.mjs --self-test` after prompt or reference changes.

## Installation

```bash
npx octocode skill --name octocode-research
```
