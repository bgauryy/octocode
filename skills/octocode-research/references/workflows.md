# Workflows

Load after `references/algorithm.md` and `references/problem-framing.md` to pick one mode-specific route. Load `references/octocode.md` only when transport or command syntax is unclear.

| File | Use for | Eval |
|---|---|---|
| `references/workflow-local.md` | running repo, checkout, artifact, installed dependency | `local-research` |
| `references/workflow-external.md` | remote repo/PR/package/upstream | `external-research` |
| `references/workflow-combination.md` | local clue → upstream, or remote code needing local-grade AST/LSP/negative proof | `campaign-combination` |
| `references/workflow-debug.md` | failure, error, behavior/root cause | `code-investigation`, `oql-graph-proof` |
| `references/workflow-change.md` | implement/migrate/patch (new or changed behavior) | `change-mode` |
| `references/workflow-refactor.md` | reshape structure/names/modules/layout while preserving contracts | `refactor-mode` |
| `references/workflow-pr-review.md` | PR URL/#N/safe-to-merge, local changes/diff, file review; sole Octocode review workflow | `pr-local-review` |

Rare paths: `references/long-research.md` for durable/contested decisions (`long-decision-brief`); `references/github-landscape.md` for repo ecosystems (`github-landscape`); `references/loop-mode.md` after repeated evidence/check changes (`loop-mode`). Cross-task meta (planning, measuring, subagent fan-out, efficiency): `references/researcher-mindset.md`. For divergent idea generation or build/no-build validation, stay in Research framing and decide explicitly before coding.

## Common Spine
`problem contract → classify → system model → surface plan → cheap map → anchor → exact read → stronger proof → answer/patch/review`

Name corpus and skipped surfaces: local path, repo/ref, PR, package/version, artifact, history window. Keep classification `unknown` until actual/expected/authority are grounded. Promote claims only after exact evidence plus AST/LSP/history/artifact/spec/test proof.

## Minimal Loads
| Task | References |
|---|---|
| small fact/code question | algorithm + problem-framing; add `octocode.md` if transport is unclear |
| local/external route | algorithm + problem-framing + matching local/external workflow |
| bug/root cause | algorithm + problem-framing + debug + code-research |
| feature | algorithm + problem-framing + change + code-research |
| enhancement | algorithm + problem-framing + change + code-research; baseline and target required |
| unknown class | algorithm + problem-framing; investigate actual/authority before choosing debug/change |
| PR/local review | algorithm + PR-review + `references/code-research.md`; follow its analysis/report routes |
| change | algorithm + change + `references/code-research.md`; add loop-mode after failed verification |
| refactor (structure/names/modules) | algorithm + refactor + `references/code-research.md`; hand off to change if behavior must change |
| converging / flipping evidence | algorithm + `references/loop-mode.md` |
| long decision | algorithm + long-research; add landscape only for repo ranking |

Handoff receipt: `mode | scope | active/skipped surfaces | claims/evidence/confidence/gaps | verification | next`.

Feed local dependency/error/config clues into external research; return upstream fixes/history to local proof. Debug hands to Change when edits are authorized.
PR review reuses local/external chains; Map/Validate live in `references/research-flow.md`.

After any workflow edit run `node scripts/eval-research.mjs --self-test` or its mapped `--case <id>`.
