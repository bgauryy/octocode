# Octocode Research

Investigate local code, external repositories, packages, history, failures, and reviews with exact evidence before asserting a claim or making a change.

## Use when

- You need callers, imports, paths, affected-area analysis, or safe-delete proof.
- You must locate a behavior or analyze the root cause of a failure.
- An upstream repository, package registry, commit, or pull request can answer the question.
- A planned change needs evidence before editing and verification afterward.
- You’re asked to “research this” or “use octocode.”

## Not for

- A trivial edit whose impact is already known → make the edit directly
- Writing or reviewing documentation → `octocode-documentation`
- Skill folder structure → `octocode-skills`
- Open-ended idea exploration → `octocode-brainstorming`

## Workflow

```text
FRAME → CLASSIFY → MODEL → SEARCH → READ EXACT → PROVE → DECIDE/PATCH → VERIFY
```

Search results are leads until exact bytes confirm. Findings cite repository paths, package versions, commits, or URLs with explicit confidence. Empty results describe only the searched lane.

## Install

```bash
npx -y octocode skill install octocode-research
```

## Maintainer verification

```bash
node scripts/check-description.mjs
node scripts/check-guidance.mjs --self-test
```

Then run the `octocode-skills` review against this folder.
