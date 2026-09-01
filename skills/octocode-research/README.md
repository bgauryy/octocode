# Octocode Research

Evidence-first investigation for local code, external repositories, packages, history, reviews, implementation planning, and refactors.

## Use it for

- callers, imports, dependency paths, blast radius, or safe-delete proof;
- locating behavior, mapping a system, or diagnosing a supported-contract failure;
- upstream fixes, npm packages, prior art, and ecosystem comparisons;
- planning before a change and validating callers, tests, and the final diff afterward.

Skip it when a trivial edit's impact is already known. Docs writing belongs to `octocode-documentation`; skill folders to `octocode-skills`; open-ended product exploration to `octocode-brainstorming`.

## How it works

`FRAME → CLASSIFY → MODEL → SEARCH → READ EXACT → PROVE → DECIDE/PATCH → VERIFY`

Depth follows claim risk. Exact anchors (`file:line`, repository path, package/version, PR, commit, or URL) carry `confirmed`, `likely`, `uncertain`, or `weak` confidence. Search snippets are leads; empty results describe one searched lane, not universal absence.

Routes cover local, external, combined local↔remote, debug/RCA, behavioral change, refactor, and PR/diff review. Rare routes cover ecosystem ranking, durable briefs, and convergence loops. The agent loads only the active route and proof guidance.

Octocode MCP is preferred. This monorepo uses the built CLI; an installed skill uses `npx octocode`. Both expose local search/read/graph, LSP semantics, GitHub code/history, cloning, and npm lookup. Disabled surfaces are reported, never simulated.

## Install

```bash
npx octocode skill --name octocode-research
```

Maintainers: keep tactics and report contracts in `SKILL.md`/`references/`; verify description changes with `node scripts/check-description.mjs`.
