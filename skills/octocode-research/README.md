# Octocode Research

Answers technical questions with evidence instead of vibes. Investigation, review, implementation, refactor analysis, prior-art mapping, and loops when one pass isn't enough.

## When to use it

Reach for it when code must be checked, not assumed:

- **Connections** — who calls this, what imports it, what breaks if I change it?
- Where does this behavior live? How does this system work? Why does it fail?
- Safe to delete? Dependency still used? Has someone already solved this?
- **Outward** — external repositories, npm packages, cross-repo connections, general research.
- **Before writing** — plan the flow: touch points, blast radius, patterns to copy.
- **After writing** — validate it landed: callers hold, tests cover it, diff is safe to merge.
- Anytime someone asks to **research** something, or asks for **octocode**.

**Skip it** for a trivial edit whose blast radius is already known. Blunt critique → `octocode-roast`; decision before coding → `octocode-rfc-generator`; worth building at all → `octocode-brainstorming`.

## The problem it solves

Technical work fails when an agent treats a search snippet as proof, edits before understanding blast radius, or reports confidence without showing where it came from. A codebase rarely rewards a single lucky query.

It also fails when everything labeled "bug" gets debugged as a defect, features are assigned fictional root causes, or enhancements start with no baseline. This skill defines actual versus desired behavior first and classifies the work as bug, feature, enhancement, or unknown — then searches cheaply, reads exact evidence, and either recommends a path or makes a scoped change with verification.

## Operating model

```text
FRAME -> CLASSIFY -> MODEL -> SEARCH -> READ EXACT -> PROVE -> DECIDE/PATCH -> VERIFY
```

That's the shape, not a checklist. Depth scales to the claim: a small lookup gets a cheap read and an honest confidence label; a delete, a merge verdict, or a root cause earns the whole ladder.

Every claim carries an exact anchor (`file:line`, repo path, package id, PR number, commit, URL) and a confidence label (confirmed / likely / uncertain / weak). Alternate explanations stay alive until evidence kills them. Empty results are reported as "this lane can't see it," never as "it isn't there."

## Workflows

Seven routes, one per situation: local checkout, remote repo, local↔remote combination, debug/root-cause, change, refactor, and PR/diff review. Rare paths cover ecosystem ranking, durable decision briefs, and convergence loops. The agent loads one route plus the proof ladder — not everything.

## Tooling

Uses Octocode **MCP tools** when they're exposed. In this monorepo it uses the built local CLI; an installed standalone skill can use **`npx octocode`**. Both expose the same 17 public schemas and availability metadata:

- local: search, find, read, tree, and graph analysis (dependencies, dependents, paths, cycles, reachability, dead-code candidates)
- semantics: LSP definitions, references, callers, callees, symbols, diagnostics
- GitHub: code, repos, files, structure, PRs, issues, commits, releases, discussions, clone
- packages: npm lookup

Execution gates (`ghListReleases` via `ENABLE_RELEASES=1`, `ghSearchDiscussions` via `ENABLE_DISCUSSIONS=1`) and MCP-side gates (`ENABLE_LOCAL`, `ENABLE_CLONE`) are documented in `references/octocode.md`. A disabled surface is reported as skipped, never faked.

## Installation

```bash
npx octocode skill --name octocode-research
```

## Maintainer notes

Keep this README about the discipline users should expect. Mode-specific tactics, tool routing, and report formats belong in `SKILL.md` and the focused references.

Before publishing a change:

```bash
node scripts/check-description.mjs      # description contract (--help, --json)
```
