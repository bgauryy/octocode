# Workflows

Index for picking the efficient Octocode route.
Read `algorithm.md` first for routing and evidence grades.
Read `octocode.md` when transport or command syntax is unclear.
Each workflow below lives in its own file; load only what the task needs.

## Workflow Files

| File | Use for | Eval case(s) |
|---|---|---|
| `workflow-local.md` | running repo, local checkout, local artifact, or installed dependency is the truth | — |
| `workflow-external.md` | remote repo, PR, package, prior-art question, or upstream dependency not present locally | — |
| `workflow-debug.md` | why behavior changed, a test failed, an error appears, or a bug exists | `code-investigation`, `oql-graph-proof` |
| `workflow-change.md` | implement, refactor, migrate, or patch after evidence gathering | `change-mode` |
| `workflow-pr-review.md` | review PR, safe to merge, review my changes/diff, local file-scope review | `pr-local-review` |

Rare paths, skip by default: long/contested/public decision briefs -> `long-research.md`; repo ecosystem comparisons -> `github-landscape.md`; repeated Act->Observe->Learn convergence -> `loop-mode.md`.

Validate any workflow file edit with `node scripts/eval-research.mjs --self-test` (all cases) or `--case <id>` for the row above; case prompts and pass criteria live in `evals/prompts.md`/`evals/cases.json`.

## Common Spine

```text
scope -> surface plan -> cheap map -> anchor -> exact read -> stronger proof -> answer/patch/review
```

Start with the corpus and skipped surfaces: local path, owner/repo/ref, PR number, package/version, artifact, history window, and why each surface is active or skipped.
Prefer cheap orientation before deep reads. Promote claims only after exact evidence plus at least one stronger lane: AST shape, LSP identity, history/PR intent, artifact metadata, docs/specs, or tests.

## Minimal Reads

Use the lightest reference stack that can answer the task:

| Task | Load |
|---|---|
| Small factual/code question | `algorithm.md`; add `octocode.md` only if transport is unclear |
| Local or external route choice | `algorithm.md` + `workflow-local.md` and/or `workflow-external.md` |
| Bug or root cause | `algorithm.md` + `workflow-debug.md` + `code-research.md` |
| PR or local review | `algorithm.md` + `workflow-pr-review.md` + `code-research.md` |
| Implementation/change | `algorithm.md` + `workflow-change.md` + `code-research.md`; add `loop-mode.md` after failed verification |
| Long/contested decision | `algorithm.md` + `long-research.md`; add `github-landscape.md` only for repo ecosystem ranking |

Default receipt for handoffs and subagents:

```text
mode | scope | active/skipped surfaces | claims with evidence/confidence/gaps | verification | next step
```

## Cross-Workflow Notes

- Cross-pollinate: a local clue (dependency name, error string, config key) feeds `workflow-external.md`; an external fact (upstream fix, PR intent) feeds back into `workflow-local.md`.
- `workflow-debug.md` hands off to `workflow-change.md` once a fix requires an edit.
- `workflow-pr-review.md` reuses `workflow-local.md`/`workflow-external.md` tool chains for its blast-radius tracing — it does not duplicate them.
- Map/Validate flows (landscape, prior art, whether to build) are not workflow-*.md files; they live in `research-flow.md`, with `github-landscape.md`/`long-research.md` as extensions.
