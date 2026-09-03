---
name: octocode-research
description: "Use when a code claim must be checked, not assumed: trace callers/imports/cross-repo wiring and change impact; locate behavior; map a system; diagnose failures/RCA; inspect external repositories, npm packages, upstream work, or prior art; plan before coding and validate afterward. Triggers on 'research this' or 'use octocode'. Return exact file:line/PR/commit evidence with confidence. Skip trivial edits whose blast radius is already known. Not for writing docs → octocode-documentation, or skill folders → octocode-skills."
---

# Octocode Research

Evidence before assertion: find an anchor, read exact bytes, prove the claim, then answer, or patch.

Flow: `FRAME → CLASSIFY → MODEL → SEARCH → READ EXACT → PROVE → DECIDE/PATCH → VERIFY`.

Scale depth to risk. A lookup needs one exact read and honest confidence; deletion, merge verdicts, and root cause need the full proof ladder. Workspace reports default to `<workspace>/.octocode/octocode-research/`, scratch to `<workspace>/.octocode/tmp/octocode-research/`; chat findings stay in chat, approved source edits keep their paths, and artifacts never fall back to user-level Octocode home.

## Gates

- Start with corpus, actual vs needed, task class, mode, and active/skipped surfaces. Call something a bug only when evidence shows a supported contract violation.
- Root cause requires mechanism, trigger, violated contract, divergence boundary, and one disconfirmed alternate.
- For nontrivial claims, inspect two of structure, stream, and connections. Snippets are leads; empty proves only that the named lane found nothing.
- Track `claim → evidence → confidence → next check`; cite exact anchors and checks that ran.
- Ask before public/broad contracts, deletes, or renames, thin-evidence changes, a third unrelated search space, cloning/running untrusted code, or writing an unrequested artifact.

Stop when evidence answers the framed question and kills the alternate. No cheap check can change the conclusion. The default 3–5 decisive iterations/~15-minute budget is spent. Recent iterations change no state. Failures remain thin. A decision belongs to you. A gate blocks. Or a skill edit measures flat/worse. Report gaps without inflating confidence.

## Routes

Start with `references/algorithm.md` for routing/proof grades and `references/problem-framing.md` for bug/feature/enhancement/unknown. Use `references/workflows.md` when route choice, load budget, or a handoff receipt is unclear.

At FRAME/CLASSIFY/MODEL, ground the problem contract, and load-bearing system path; SEARCH/READ EXACT/PROVE follow the chosen route; DECIDE/PATCH and VERIFY use that route's output/check contract.

- When the corpus is a checkout/package, load `references/workflow-local.md`; for an external repository/package/upstream, load `references/workflow-external.md`; for local↔remote or remote AST/LSP proof, load `references/workflow-combination.md`.
- When investigating failure/RCA, load `references/workflow-debug.md`; for behavioral implementation/migration, load `references/workflow-change.md`; for a behavior-preserving reshape, load `references/workflow-refactor.md`.
- PR/local diff review → `references/workflow-pr-review.md`, then `references/workflow-pr-review-analysis.md`, then `references/workflow-pr-review-report.md`.
- When proving callers/imports/paths/cycles/reachability/deletion/architecture, load `references/code-research.md`; for Map/Validate/Investigate/Plan across surfaces, load `references/research-flow.md`.
- When comparing several repos/packages, load `references/github-landscape.md`; for shifting evidence, load `references/loop-mode.md`; for a durable contested brief, load `references/long-research.md`; for campaign budgets/fan-out, load `references/researcher-mindset.md`.

Load only the references earned by the current step. `references/octocode.md` owns interfaces, schemas, auth, gates, materialization, diagnostics, and exit codes. `references/improve-loop.md` owns accept/revert when this skill changes.

## Tools and output

Prefer Octocode MCP. In this monorepo use `node packages/octocode/out/octocode.js`; installed skills use `npx octocode`. Read `$OCTO tools <name> --scheme --json --compact` before `$OCTO tools <name> --queries '<json>' --compact`; batch up to five independent queries and follow returned continuations. Use `localAnalyzeGraph` for file topology and LSP for symbol identity.

Return `Finding · Evidence · Confidence · Next`; decisions add verdict, risks, exact anchors, verification, and the smallest safe fix. Related: `octocode-brainstorming`, `octocode-rfc-generator`, `octocode-eval-benchmark`, `octocode-documentation`, `octocode-skills`, `octocode-subagent`, `octocode-roast`.

After editing this skill, run `node scripts/check-description.mjs`; accept/revert through `references/improve-loop.md`.
