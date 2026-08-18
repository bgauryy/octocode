# Workflows

Load after `references/algorithm.md` and `references/problem-framing.md`. Pick **one** route and follow it; load `references/octocode.md` only when transport or syntax is unclear.

## What each route is for

| Route | Use it when | What it gives you | Eval |
|---|---|---|---|
| `references/workflow-local.md` | The running repo, checkout, artifact, or installed dependency is the source of truth | Local spine: tree → search → exact read → LSP. `node_modules` is ground truth for what actually runs | `local-research` |
| `references/workflow-external.md` | The corpus is a remote repo, PR, package, or upstream dependency | Remote spine: discovery → structure → anchors → exact read → history. Treats GitHub zeros as provider blind spots, not absence | `external-research` |
| `references/workflow-combination.md` | One surface can't answer — a local clue points upstream, or remote code needs AST/LSP/negative proof | The bridge: materialize remote code so the local loop runs on it unmodified | `campaign-combination` |
| `references/workflow-debug.md` | Something fails and you need the actual cause | Two-hypothesis discipline, divergence boundary, counterfactual proof, root-cause receipt | `code-investigation`, `dead-code-proof` |
| `references/workflow-change.md` | You're implementing, migrating, or patching **behavior** | Blast radius before edit, smallest patch boundary, verification that actually ran | `change-mode` |
| `references/workflow-refactor.md` | You're reshaping names/modules/layout and behavior must **not** change | Skeleton → contracts → blast → big-to-small tasks → layered verify | `refactor-mode` |
| `references/workflow-pr-review.md` | A PR, a diff, or "is this safe to merge" | Risk sizing, domain order, finding shape, verification-gated recommendation. Sole review workflow | `pr-local-review` |

## Common spine

```text
problem contract → classify → system model → surface plan → cheap map → anchor → exact read → stronger proof → answer/patch/review
```

Name the corpus and the skipped surfaces: local path, repo/ref, PR, package/version, artifact, history window. Keep the class `unknown` until actual, expected, and authority are grounded. Promote a claim only after exact evidence plus AST/LSP/history/spec/test proof.

## How much to load

Scale to the claim. Loading everything is a failure mode.

| Task | Load |
|---|---|
| small fact / code question | algorithm + problem-framing; add `octocode.md` if transport is unclear |
| local or external lookup | algorithm + problem-framing + the matching local/external route |
| bug / root cause | algorithm + problem-framing + debug + `references/code-research.md` |
| feature | algorithm + problem-framing + change + `references/code-research.md` |
| enhancement | same as feature, plus a measured baseline and target |
| unknown class | algorithm + problem-framing; ground actual/authority before choosing debug or change |
| PR / local review | algorithm + PR-review + `references/code-research.md`; follow its analysis and report routes |
| refactor | algorithm + refactor + `references/code-research.md`; hand to change if behavior must move |
| evidence keeps flipping | add `references/loop-mode.md` |
| long or contested decision | algorithm + `references/long-research.md`; add `references/github-landscape.md` only for repo ranking |

Cross-task meta — planning, budgets, measuring progress, subagent fan-out: `references/researcher-mindset.md`. Map and Validate execution live in `references/research-flow.md`.

## Switching routes

Routes hand off; they don't nest. Debug hands to Change once edits are authorized. Change hands to `loop-mode.md` when verification keeps failing. Refactor hands to Change if behavior has to move. Local and external cross-pollinate: local dependency, error, and config clues feed outward; upstream fixes and PR intent come back for local confirmation.

Handoff receipt: `mode | scope | active/skipped surfaces | claims/evidence/confidence/gaps | verification | next`.

After any workflow edit run `node scripts/eval-research.mjs --self-test`, or `--case <id>` for the route you touched.
