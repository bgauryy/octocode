---
name: octocode-research
description: "Use when a technical question or code change needs evidence before conclusions: find or locate behavior, explain systems, diagnose failures, review diffs, validate dependencies or prior art, prove dead code, plan refactors, or ship the smallest verified fix."
---

# Octocode Research

Evidence-first technical work. Flow: `FRAME → CLASSIFY → MODEL → SEARCH → READ EXACT → PROVE → DECIDE/PATCH → VERIFY`.
Modes: investigate, review, change, refactor, prior-art validation, and evidence loops. Task class (bug/feature/enhancement/unknown) is separate from mode.

## Lobby rules
1. State corpus, observed/desired behavior, authority, task class, mode, and active/skipped surfaces.
2. Call it a bug only when evidence proves a supported contract was violated.
3. Root cause needs mechanism, trigger, violated contract, divergence boundary, and disconfirming or counterfactual proof.
4. Use the strongest available handle; for nontrivial claims inspect at least two of structure, stream, and connections.
5. Track `claim → evidence → confidence → next check`; cite exact anchors and checks that actually ran.
6. Ask before broad contracts, deletion/rename, thin evidence, or three unrelated search spaces; patch only after proof.

## Smart routes — load only what the current step needs
- Start every task with `references/algorithm.md`, then `references/problem-framing.md` — choose evidence grade, class, proof, and success before searching.
- When choosing a workflow, load `references/workflows.md`; then use `references/workflow-local.md`, `references/workflow-external.md`, `references/workflow-debug.md`, `references/workflow-change.md`, or `references/workflow-refactor.md` — get the corpus/mode-specific gates without mixing flows.
- When reviewing a diff/PR, load `references/workflow-pr-review.md`; use `references/workflow-pr-review-analysis.md` during inspection and `references/workflow-pr-review-report.md` during reporting — separate evidence collection from verdict.
- When modes must combine, load `references/workflow-combination.md` — order dependent flows and keep one claim ledger.
- When investigating or changing code, load `references/code-research.md`; for broader validation load `references/research-flow.md` — choose code semantics or general-source proof deliberately.
- When planning progress or fan-out, load `references/researcher-mindset.md`; when evidence shifts, load `references/loop-mode.md` — budget checks and converge instead of searching indefinitely.
- When mapping ecosystems, load `references/github-landscape.md`; when a decision is deep or contested, load `references/long-research.md` — rank prior art or produce a durable brief.
- When command, MCP, or schema details matter, load `references/octocode.md` — use verified transport syntax; when improving this skill, prefer `octocode-eval` (else `references/improve-loop.md`) — enforce an accept/revert gate.

## Related routes
- Use `octocode-awareness` for shared-repo coordination, locks, verification, and durable lessons.
- Use `octocode-brainstorming` when worth-building is unresolved; `octocode-eval` for goal→KPI / keep-discard; `octocode-rfc-generator` for design contracts; `octocode-skills` for skill-folder lifecycle.
- Use `octocode-subagent` for bounded fan-out; `octocode-roast` for critique tone.

## Script and output
- When changing this skill, run `scripts/eval-research.mjs` for the matching case — catch routing regressions.
- Quick output: `Finding`, `Evidence`, `Confidence`, `Next`; decisions add verdict, risks, exact anchors, verification, and smallest safe fix.
