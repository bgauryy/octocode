# Workflows

Load after `references/algorithm.md` to pick one mode-specific route. Load `references/octocode.md` only when transport or command syntax is unclear.

| File | Use for | Eval |
|---|---|---|
| `references/workflow-local.md` | running repo, checkout, artifact, installed dependency | — |
| `references/workflow-external.md` | remote repo/PR/package/upstream | — |
| `references/workflow-combination.md` | local clue → upstream, or remote code needing local-grade AST/LSP/negative proof | `campaign-combination` |
| `references/workflow-debug.md` | failure, error, behavior/root cause | `code-investigation`, `oql-graph-proof` |
| `references/workflow-change.md` | implement/migrate/patch (new or changed behavior) | `change-mode` |
| `references/workflow-refactor.md` | reshape structure/names/modules/layout while preserving contracts | `refactor-mode` |
| `references/workflow-pr-review.md` | PR URL/#N/safe-to-merge, local changes/diff, file review; sole Octocode review workflow | `pr-local-review` |

Rare paths: `references/long-research.md` for durable/contested decisions; `references/github-landscape.md` for repo ecosystems; `references/loop-mode.md` after repeated evidence/check changes. Cross-task meta (planning, measuring, subagent fan-out, efficiency): `references/researcher-mindset.md`. For divergent idea generation or build/no-build validation, switch to the `octocode-brainstorming` skill.

## Common Spine
`scope → surface plan → cheap map → anchor → exact read → stronger proof → answer/patch/review`

Name corpus and skipped surfaces: local path, repo/ref, PR, package/version, artifact, history window. Promote claims only after exact evidence plus AST/LSP/history/artifact/spec/test proof.

## Minimal Loads
| Task | References |
|---|---|
| small fact/code question | `references/algorithm.md`; add `references/octocode.md` if transport is unclear |
| local/external route | algorithm + matching local/external workflow |
| bug/root cause | algorithm + debug + `references/code-research.md` |
| PR/local review | algorithm + PR-review + `references/code-research.md`; follow its analysis/report routes |
| change | algorithm + change + `references/code-research.md`; add loop-mode after failed verification |
| refactor (structure/names/modules) | algorithm + refactor + `references/code-research.md`; hand off to change if behavior must change |
| long decision | algorithm + long-research; add landscape only for repo ranking |

Handoff receipt: `mode | scope | active/skipped surfaces | claims/evidence/confidence/gaps | verification | next`.

Feed local dependency/error/config clues into external research; return upstream fixes/history to local proof. Debug hands to Change when edits are authorized.
PR review reuses local/external chains; Map/Validate live in `references/research-flow.md`.

After any workflow edit run `node scripts/eval-research.mjs --self-test` or its mapped `--case <id>`.
