# Octocode Research

Investigate local code, external repositories, packages, history, failures, reviews, and implementation plans with exact evidence.

## Use when

- You need callers, imports, paths, affected-area analysis, or safe-delete proof.
- You must locate behavior or analyze the root cause of a failure.
- An upstream repository, npm package, commit, or pull request can answer the question.
- A planned change needs evidence before editing and verification afterward.

Skip this skill when a trivial edit's impact is already known. Documentation writing belongs to `octocode-documentation`; skill folders belong to `octocode-skills`; open-ended idea exploration belongs to `octocode-brainstorming`.

## Workflow

```text
FRAME → CLASSIFY → MODEL → SEARCH → READ EXACT → PROVE → DECIDE/PATCH → VERIFY
```

Search results are leads. Findings use exact repository paths, package versions, commits, pull requests, or URLs with explicit confidence. Empty results describe only the searched lane.

## Tools

Prefer Octocode MCP tools. The monorepo uses its built CLI; installed skills use `npx octocode`. Use graph operations for file topology and LSP for symbol identity.

## Install

```bash
npx octocode skill install octocode-research --platform codex
```

## Maintainer verification

```bash
node scripts/check-description.mjs
```

Then run the `octocode-skills` review against this folder.
