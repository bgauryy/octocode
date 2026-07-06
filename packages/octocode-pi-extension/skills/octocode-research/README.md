# Octocode Research

`octocode-research` gives an agent the discipline to answer technical questions with evidence instead of vibes. It covers investigation, planning, review, implementation, refactor analysis, artifact inspection, prior-art mapping, and repeated loops when one pass is not enough.

Use it when the answer should be grounded in code, history, package data, exact files, or verifiable behavior.

## The Problem

Technical work fails when an agent treats search snippets as proof, edits before understanding blast radius, or reports confidence without showing where it came from. A codebase rarely rewards a single lucky query.

This skill makes the agent scope the question, search cheaply first, read exact evidence, validate findings, and either recommend a path or make a scoped change with verification.

## Capabilities

- Mode selection for map, validate, investigate, plan, review, change, and loop tasks.
- Evidence surfaces that can include local code, GitHub, npm, PR history, artifacts, binaries, docs, specs, and papers.
- Exact anchors such as `file:line`, repo path, package id, PR number, commit, artifact fact, or fetched URL.
- AST, structural search, LSP, history, package, GitHub, and local-file workflows through Octocode when available.
- Confidence labels for confirmed, likely, uncertain, and weak claims.
- Finding checks that keep alternate explanations alive until evidence resolves them.
- Review output ordered by severity, impact, confidence, and citation quality.
- Change output that stays scoped and reports the verification that actually ran.

## Operating Model

The workflow is:

```text
SCOPE -> SEARCH -> READ EXACT -> VALIDATE -> DECIDE/PATCH -> VERIFY
```

The agent starts by naming the corpus, question, mode, and active evidence surfaces. It then uses cheap discovery to find anchors, reads exact slices once anchors appear, validates claims against stronger evidence, and only then decides, plans, reviews, or patches.

For open-ended questions, the skill loops: act, observe, learn, and repeat until evidence converges or the remaining gap is clear enough to report honestly.

## User Experience

Users should see a concise answer with proof. A good research response says what was checked, what was found, how confident the agent is, and what the next step should be. When the task is a review, findings lead. When the task is a change, the patch stays as small as the evidence allows.

The skill is the default technical workhorse for Octocode because it can move from question to plan to verified edit without losing the evidence trail.

## Installation

Install the published skill with:

```bash
npx octocode skill --name octocode-research
```

## Maintainer Notes

Keep this README about the research discipline users should expect. Keep mode-specific tactics, exact tool routing, long-report behavior, and ecosystem-comparison details in the agent-facing skill file and focused references.
