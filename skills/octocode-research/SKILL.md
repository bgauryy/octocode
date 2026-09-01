---
name: octocode-research
description: "Use when code must be checked, not assumed: trace connections (callers, imports, cross-repo wiring, what breaks if I change it), locate behavior, map a system, diagnose a failure, RCA. Also for external repositories, npm packages, upstream/prior art, and general research. Plan a coding flow before writing, validate it after. Triggers on 'research this' or 'use octocode'. Gives exact file:line/PR/commit evidence with confidence. Skip trivial edits whose blast radius is already known. Not for authoring or copyediting the docs themselves, or for building skill folders: docs deliverable → octocode-documentation; SKILL.md folders → octocode-skills."
---

# Octocode Research

Evidence before assertion. Find the anchor, read the exact bytes, prove the claim, then answer or patch.

```text
FRAME → CLASSIFY → MODEL → SEARCH → READ EXACT → PROVE → DECIDE/PATCH → VERIFY
```

That's the full shape, not a checklist to march through. Scale it to the claim: a small lookup gets a cheap read and an honest confidence label; a delete, a merge verdict, or a root cause earns the whole ladder. Skip stages you can already answer — but say which ones you skipped.

Workspace output contract: chat-only findings stay in chat. New decision briefs, evidence packets, or unnamed reports default to `<workspace>/.octocode/octocode-research/`; scratch data uses `<workspace>/.octocode/tmp/octocode-research/`. User-approved source edits keep their named paths. Never fall back to a user-level Octocode home for artifacts.

## The rules

1. Open with one line: corpus, actual vs desired, task class, mode, surfaces used and skipped.
2. Call it a bug only when evidence shows a supported contract was violated.
3. Root cause needs mechanism, trigger, violated contract, divergence boundary, and a killed alternate.
4. Use the strongest handle you already hold. For nontrivial claims check two of: structure, stream, connections.
5. A snippet is a lead. Empty means *this lane can't see it*, not *it isn't there*. Say which you mean.
6. Track `claim → evidence → confidence → next check`. Cite exact anchors, and only checks that actually ran.
7. Ask before broad contracts, deletes/renames, thin evidence, or a third unrelated search space. Patch after proof.

Stop when: grounded evidence answers the framed question and the alternate is dead; no cheap next step can change the conclusion; the budget is hit (default 3-5 decisive iterations or ~15 minutes); the last iterations changed no state; retries stay thin, or a license/product/architecture call belongs to the user; a gate blocks (broad contract, delete/rename, clone or run untrusted code, unapproved artifact write); or a skill edit measured flat/worse — revert through `references/improve-loop.md`. Report the remaining gaps instead of padding certainty.

## Workflows

Start with `references/algorithm.md` (routing, evidence grades) and `references/problem-framing.md` (is this a bug, feature, enhancement, or still unknown?). Then pick one route by what you're looking at — load `references/workflows.md` when you need the per-route detail, the load budget per task size, or the handoff receipt between routes:

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
| Trace connections — callers, imports, dependency paths, cycles, reachability | `references/code-research.md` |

Proof depth for any of them: `references/code-research.md`. General research — Map / Validate / Investigate / Plan across code, packages, docs, and history: `references/research-flow.md`.

Review runs in three parts: use `references/workflow-pr-review.md` for target, guidelines, and risk sizing; then `references/workflow-pr-review-analysis.md` for sizing depth, flow proof, and finding shape; then `references/workflow-pr-review-report.md` for the verification-gated recommendation and the optional written document.

Reach for these only when they earn it: `references/loop-mode.md` (evidence keeps flipping), `references/long-research.md` (durable decision brief), `references/researcher-mindset.md` (budgets, fan-out, campaign planning).

Load a reference when the current step needs it. Loading all of them is a failure mode.

## Tooling

Prefer Octocode **MCP tools** when exposed. In this monorepo use the built local CLI; from an installed standalone skill use `npx octocode`. Both expose the same 10 input contracts.

```bash
OCTO='node packages/octocode/out/octocode.js'          # monorepo; use npx octocode outside it
$OCTO context --minimal                                # what's available
$OCTO tools <name> --scheme --json --compact           # read fields — never guess
$OCTO tools <name> --queries '<json>' --compact        # run it
```

Batch up to five queries per call. Orient cheap (tree, discovery) before exact reads. Use `localAnalyzeGraph` for repository file topology and LSP for symbol identity; follow returned `next.*` and cursors instead of re-deriving them.

Read `references/octocode.md` when transport, tool choice, auth, gates (`ENABLE_LOCAL`, `ENABLE_CLONE`), materialization, diagnostics, or exit codes are unclear.

## Output

`Finding · Evidence · Confidence · Next`. Decisions add verdict, risks, exact anchors, verification, and the smallest safe fix. Report gaps instead of padding certainty.

## Related

`octocode-brainstorming` (worth building?) · `octocode-rfc-generator` (design contract) · `octocode-eval-benchmark` (goal→KPI) · `octocode-documentation` (docs deliverable) · `octocode-skills` (skill folders) · `octocode-subagent` (fan-out) · `octocode-roast` (critique tone).

When changing this skill, run `node scripts/check-description.mjs` (description contract; `--help` for flags) and gate accept/revert with `references/improve-loop.md`.
