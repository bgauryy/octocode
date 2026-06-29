# Octocode

`octocode` gives an agent a fast path into Octocode itself. It is the quick lookup skill for code, repositories, packages, pull requests, symbols, and artifacts when the user needs a focused answer rather than a full investigation.

The skill is small on purpose. It helps the agent choose the right Octocode surface, read the exact thing behind a lead, and answer with anchors the user can follow.

## The Problem

Small code questions can become noisy. An agent may search too broadly, guess a raw tool schema, or paste a pile of results instead of reading the file that matters.

This skill keeps quick lookups quick. It orients cheaply, carries returned paths and refs forward, promotes only exact evidence to proof, and escalates to deeper skills when the task grows.

## Capabilities

- Transport choice between exposed Octocode MCP tools and the CLI.
- Local and GitHub code search with exact file reads.
- Repository, package, pull request, commit, and history lookup.
- Symbol and semantic navigation through AST or LSP-backed surfaces.
- Archive, binary, generated-artifact, and string inspection.
- Schema-aware raw tool use when the quick command surface is not enough.
- Concise answers that separate leads from proof.

## Operating Model

The workflow is:

```text
ORIENT -> SEARCH -> READ EXACT -> PROVE -> ANSWER
```

The agent begins with the cheapest useful orientation: a tree, a path search, a repository lookup, a package result, or a symbol listing. It then deep-reads only the relevant source and carries anchors such as `file:line`, repo ref, package id, PR number, or local clone path into the final answer.

## User Experience

For users, `octocode` should feel like asking a precise codebase question and getting a precise answer. It is best for "where is this implemented?", "what does this package expose?", "show me the file behind this behavior", or "inspect this artifact."

When the answer reveals a deeper root-cause, refactor, review, or implementation task, the agent should move to `octocode-research` instead of stretching this quick lookup mode too far.

## Installation

Install the published skill with:

```bash
npx octocode skill --name octocode
```

## Maintainer Notes

Keep this README focused on the quick-lookup experience and evidence anchors. Keep detailed transport setup, command recipes, and edge-case routing in the agent-facing skill file and references.
