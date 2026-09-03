# Workflows

Load when the task is framed and classified and one route has to be picked. Why: routes don't nest — take exactly one, after `references/algorithm.md` and `references/problem-framing.md`, and load `references/octocode.md` only when transport or syntax is unclear. <!-- style-lint: ignore-line passive-voice -->

## What each route is for

| Route | Use it when | What it gives you |
|---|---|---|
| `references/workflow-local.md` | The running repository, checkout, artifact, or installed dependency is the source of truth | Local spine: tree → search → exact read → graph for file topology → LSP for symbol identity. `node_modules` is ground truth for what runs |
| `references/workflow-external.md` | The corpus is a remote repository, PR, package, or upstream dependency | Remote spine: discovery → structure → anchors → exact read → history. Treats GitHub zeros as provider blind spots, not absence |
| `references/workflow-combination.md` | One surface can't answer — a local clue points upstream, or remote code needs AST/LSP/negative proof | The bridge: materialize remote code so the local loop runs on it unmodified |
| `references/workflow-debug.md` | Something fails and you need the actual cause | Two-hypothesis discipline, divergence boundary, counterfactual proof, root-cause receipt |
| `references/workflow-change.md` | You're implementing, migrating, or patching **behavior** | Affected scope before edit, smallest patch boundary, verification that ran |
| `references/workflow-refactor.md` | You're reshaping names/modules/layout and behavior must **not** change | Skeleton → contracts → blast → big-to-small tasks → layered verify |
| `references/workflow-pr-review.md` | A PR, a diff, or "is this safe to merge" | Risk sizing, domain order, finding shape, verification-gated recommendation. Sole review workflow |

## Common spine

```text
problem contract → classify → system model → surface plan → cheap map → anchor → exact read → stronger proof → answer/patch/review
```

Name the corpus and the skipped surfaces: local path, repository/ref, PR, package/version, artifact, history window. Keep the class `unknown` until actual, expected, and authority are grounded. Promote a claim only after exact evidence plus AST/LSP/history/spec/test proof. <!-- style-lint: ignore-line passive-voice -->

## How much to load

Scale to the claim. Loading everything is a failure mode.

| Task | Load |
|---|---|
| small fact / code question | algorithm + problem-framing; add `octocode.md` if transport is unclear |
| local or external lookup | algorithm + problem-framing + the matching local/external route |
| bug / root cause | algorithm + problem-framing + debug + `references/code-research.md` |
| feature | algorithm + problem-framing + change + `references/code-research.md` |
| enhancement | same as feature, plus a measured baseline, and target |
| unknown class | algorithm + problem-framing; ground actual/authority before choosing debug or change |
| PR / local review | algorithm + PR-review + `references/code-research.md`; follow its analysis and report routes |
| refactor | algorithm + refactor + `references/code-research.md`; hand to change if behavior must move |
| evidence keeps flipping | add `references/loop-mode.md` |
| long or contested decision | algorithm + `references/long-research.md`; add `references/github-landscape.md` only for repository ranking |

Cross-task meta — planning, budgets, measuring progress, subagent fan-out: `references/researcher-mindset.md`. Map and Validate execution live in `references/research-flow.md`.

## Switch routes

Routes hand off; they don't nest. Debug hands to Change once edits are authorized. Change hands to `references/loop-mode.md` when verification keeps failing. Refactor hands to Change if behavior has to move. Local and external cross-pollinate: local dependency, error, and config clues feed outward; upstream fixes and PR intent come back for local confirmation. <!-- style-lint: ignore-line passive-voice -->

Handoff receipt: `mode | scope | active/skipped surfaces | claims/evidence/confidence/gaps | verification | next`.
