# Analyze a change and prove findings

Load for proof by change type, parallel review lanes or requested repairs. Pin and read the target first; the entry owns the review loop and verdict.

## Rank and order

Depth follows affected contracts: a small diff can require a full trace. State unread affected scope. Prioritize security, correctness, flow, architecture, performance, errors and maintainability by the change's risk.

| Change or suspected finding | Prove with |
|---|---|
| Changed signature | `lspSearch` callers, then read the call sites |
| New function | Its callees |
| Changed type | `lspSearch` references |
| Removed export | Graph dependents when listed, LSP references, plus runtime registration, configuration, tests and external consumers |
| Module reshape | Cycles before and after |
| Security sink | Sink shape, exact read, sources and callers, guard check |
| Test gap | Changed symbol, no test references, nearby test read |
| New or changed test on a bug-fix or behavior-change PR | [Test at base](#test-at-base): passing there means it does not cover the change. Characterization tests on a refactor pass on both by design |
| Diff vs stated intent | Files and lines touched against the PR's goal; unrelated refactors, renames or reformatting are a `suggestion` to split |
| Error handling and types | Added catch-all that logs and continues, `except: pass`, fallback defaults on failure; added `as any`, `as unknown as T`, non-null `!`, `# type: ignore`, `@ts-expect-error` without a stated reason |
| PR description | Each assurance ("preserves behavior", "improves robustness") maps to a check or diff anchor; an unbacked one is a `question` ([writing tells](writing-tells.md)) |
| Project policy | CONTRIBUTING and any AI-use or disclosure policy; a missing required disclosure or checklist item is a finding |
| Performance | The exact hot path; a benchmark when the claim depends on runtime |

Read the affected consumer before calling it broken; [evidence rules](evidence.md) distinguish source, semantic and runtime proof.

When an orchestrator assigns parallel lanes, size them to the diff's risk (for example Flow, Security and Errors, Architecture and Quality on a large change; one lane for a small one); each returns its findings, checked non-findings and limits. Merge by root cause and re-verify each consequential finding once. See the `octocode-workflows` composition contract.

## Test at base

In a user-provided checkout at the pinned head, or with approval, run from the directory the test command expects:

```bash
node <skill-folder>/scripts/test-at-base.mjs --base <baseCommitSha> --head <pinned head SHA> --test <test path> --cmd "<command that runs that test>" [--setup "<install or build step>"] --logs <workspace>/.octocode/octocode-pr-review/tmp/test-at-base
```

[test-at-base.mjs](../scripts/test-at-base.mjs) runs the command in a temporary git worktree at base with the checkout's test copied in, then in the checkout, and removes the worktree even on failure or interrupt. It does not modify the checkout's files; `--cmd` runs there as given. `--setup` runs only in the base tree: pass the project's install or build step when the test needs one. Options: `--help`.

| Result | Read it as |
|---|---|
| `covers:true` | Base failed, head passed. Cite it only after `base.log` shows the test itself failing (an assertion or a symbol the change adds), not missing setup |
| `covers:false`, `base.exit` 0 | Passes without the change: the test does not cover it |
| `covers:false`, `head.exit` not 0 | Fails at head: record a failed check |
| `head.dirty:true` | The head run included uncommitted edits; say so |
| Exit 1 or 2 | The `code` inside `error` names the failed step or the bad input; no coverage claim |

## Repair on request

When asked to repair findings, re-read current code, apply the smallest fix per finding, rerun the checks that proved it, and report repaired, failed and outstanding items.
