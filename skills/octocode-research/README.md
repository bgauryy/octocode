# Octocode Research

`octocode-research` gives an agent the discipline to answer technical questions with evidence instead of vibes. It covers investigation, planning, review, implementation, refactor analysis, prior-art mapping, and repeated loops when one pass is not enough.

Use it when the answer should be grounded in code, history, package data, exact files, or verifiable behavior.

## The Problem

Technical work fails when an agent treats search snippets as proof, edits before understanding blast radius, or reports confidence without showing where it came from. A codebase rarely rewards a single lucky query.

It also fails when every request labeled “bug” is debugged as a defect, features are assigned fictional root causes, or enhancements begin without a baseline. This skill first defines actual versus desired behavior and classifies the work as bug, feature, enhancement, or unknown.

The agent then searches cheaply, reads exact evidence, validates findings, and either recommends a path or makes a scoped change with verification.

## Capabilities

- Mode selection for map, validate, investigate, plan, review, change, refactor, and loop tasks.
- Problem contracts covering actual/desired behavior, authority, trigger, impact, success criteria, and non-goals.
- Evidence-based classification of bugs, features, enhancements, and unknown symptoms.
- Root-cause proof requiring mechanism, trigger, violated contract, divergence boundary, and disconfirmation.
- Evidence surfaces that can include local code, GitHub, npm, PR history, docs, specs, and papers.
- Exact anchors such as `file:line`, repo path, package id, PR number, commit, or fetched URL.
- AST, structural search, LSP, history, package, GitHub, and local-file workflows through Octocode when available.
- Confidence labels for confirmed, likely, uncertain, and weak claims.
- Finding checks that keep alternate explanations alive until evidence resolves them.
- Review output ordered by severity, impact, confidence, and citation quality.
- Change output that stays scoped and reports the verification that actually ran.
- Refactor output that maps skeleton → contracts → blast → big-to-small tasks (bulk `mv` when fit) and verifies contracts held.

## Operating Model

The workflow is:

```text
FRAME -> CLASSIFY -> MODEL -> SEARCH -> READ EXACT -> PROVE -> DECIDE/PATCH -> VERIFY
```

The agent starts by defining actual and desired behavior, the source of authority, task class, corpus, mode, and active evidence surfaces. It maps the load-bearing system path, uses cheap discovery to find anchors, reads exact slices, and only then decides, plans, reviews, or patches.

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
