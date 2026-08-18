---
name: octocode-research
description: "Use when code must be checked, not assumed: trace connections (callers, imports, cross-repo wiring, what breaks if I change it), locate behavior, map a system, diagnose a failure, RCA. Also for external repositories, npm packages, upstream/prior art, and general research. Plan a coding flow before writing, validate it after. Triggers on 'research this' or 'use octocode'. Gives exact file:line/PR/commit evidence with confidence. Skip trivial edits whose blast radius is already known"
---

# Octocode Research

Evidence before assertion. Find the anchor, read the exact bytes, prove the claim, then answer or patch.

```text
FRAME → CLASSIFY → MODEL → SEARCH → READ EXACT → PROVE → DECIDE/PATCH → VERIFY
```

That's the full shape, not a checklist to march through. Scale it to the claim: a small lookup gets a cheap read and an honest confidence label; a delete, a merge verdict, or a root cause earns the whole ladder. Skip stages you can already answer — but say which ones you skipped.

## The rules

1. Open with one line: corpus, actual vs desired, task class, mode, surfaces used and skipped.
2. Call it a bug only when evidence shows a supported contract was violated.
3. Root cause needs mechanism, trigger, violated contract, divergence boundary, and a killed alternate.
4. Use the strongest handle you already hold. For nontrivial claims check two of: structure, stream, connections.
5. A snippet is a lead. Empty means *this lane can't see it*, not *it isn't there*. Say which you mean.
6. Track `claim → evidence → confidence → next check`. Cite exact anchors, and only checks that actually ran.
7. Ask before broad contracts, deletes/renames, thin evidence, or a third unrelated search space. Patch after proof.

## Workflows

Start with `references/algorithm.md` (routing, evidence grades) and `references/problem-framing.md` (is this a bug, feature, enhancement, or still unknown?). Then pick one route by what you're looking at:

| Situation | Route |
|---|---|
| This repo, checkout, installed dependency | `references/workflow-local.md` |
| External repository, npm package, upstream project | `references/workflow-external.md` |
| Cross-repo connections, local clue → upstream, or remote code needing AST/LSP proof | `references/workflow-combination.md` |
| Rank an ecosystem — several candidate repos/packages | `references/github-landscape.md` |
| Something fails and you need the cause | `references/workflow-debug.md` |
| Plan a coding flow, implement, migrate, patch behavior | `references/workflow-change.md` |
| Reshape structure or names, keep behavior | `references/workflow-refactor.md` |
| Validate after a change; review a PR or local diff | `references/workflow-pr-review.md` |
| Trace connections — callers, imports, references, reachability | `references/code-research.md` |

Proof depth for any of them: `references/code-research.md`. General research — Map / Validate / Investigate / Plan across code, packages, docs, and history: `references/research-flow.md`.

Reach for these only when they earn it: `references/loop-mode.md` (evidence keeps flipping), `references/long-research.md` (durable decision brief), `references/researcher-mindset.md` (budgets, fan-out, campaign planning).

Load a reference when the current step needs it. Loading all of them is a failure mode.

## Tooling

Prefer Octocode **MCP tools** when exposed. Otherwise `npx octocode tools <name>` — same 15 tools, same schemas, no loss.

```bash
npx octocode context --minimal                         # what's available
npx octocode tools <name> --scheme --json --compact    # read fields — never guess
npx octocode tools <name> --queries '<json>' --compact # run it
```

Batch up to five queries per call. Orient cheap (tree, discovery) before exact reads. Follow returned `next.*` and cursors instead of re-deriving them.

Gates (`ENABLE_LOCAL`, `ENABLE_CLONE`, `ENABLE_RELEASES`), materialization, diagnostics, exit codes: `references/octocode.md`.

## Output

`Finding · Evidence · Confidence · Next`. Decisions add verdict, risks, exact anchors, verification, and the smallest safe fix. Report gaps instead of padding certainty.

## Related

`octocode-brainstorming` (worth building?) · `octocode-rfc-generator` (design contract) · `octocode-graph-eval` (goal→KPI) · `octocode-skills` (skill folders) · `octocode-subagent` (fan-out) · `octocode-roast` (critique tone).

Changing this skill: `node scripts/check-description.mjs`, then `node scripts/eval-research.mjs --self-test`. Accept/revert gate: `references/improve-loop.md`.
