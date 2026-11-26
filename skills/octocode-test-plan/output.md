# Output

<!-- report-contract {"statuses":["Ready","Draft","Blocked"],"required":["Answer","Scope","Executor contract","Features","Cases","Success criteria","Not tested","Gaps","Results format"],"anyOf":[]} -->

Lead in chat with readiness, main risk and blocking gaps; link the saved plan. Cases are unexecuted.

## Saved report

Always save at `<workspace>/.octocode/octocode-test-plan/<YYYY-MM-DD>-<slug>.md`; scratch goes in `<workspace>/.octocode/tmp/octocode-test-plan/`. `<workspace>` is the repository root, else current directory, never inside `.octocode/`. A user-named path wins.

Use the [shared header](references/report-output.md#report-header) and `question: "Test plan: <target>"`. Accepted `status` values: `Ready`, `Draft`, `Blocked`. Each means:

| Status | Meaning |
|---|---|
| `Ready` | Plan check passes; every P0 is runnable by the named executor |
| `Draft` | Named gaps remain, such as unconfirmed commands or unread code |
| `Blocked` | A missing decision, credential or environment prevents P0 execution; describe it as SBAR under Gaps |

Start `# Test plan: <target>` with the target, revision and meaning of success. Include the sections below in this order; an empty section says `None: <why>`, without filler.

| Sections, in order | How to write them |
|---|---|
| Answer, Scope | [Plan sections](references/plan-sections.md) |
| Executor contract | [Executor contract](references/plan-executor.md#executor-contract) |
| Features | [Features](references/plan-sections.md#features) |
| Cases | [Case format](references/plan-cases.md) |
| Success criteria, Not tested, Findings (optional), Gaps | [Plan sections](references/plan-sections.md#success-criteria) |
| Results format | [Results format](references/plan-executor.md#results-format), also the check to run before accepting a results file |

## Plan check

Before claiming `Ready`, run the [plan checker](scripts/plan-check.mjs) on the saved plan from the workspace, then make the judgment checks below. Fix each failure or list it under Gaps and use `Draft` (or `Blocked` for a P0 prerequisite).

```sh
node <skill-folder>/scripts/plan-check.mjs <plan.md>
```

Exit 0 (`ok: true`): the mechanical checks pass. Exit 1: every non-empty list is a failure. `duplicateIds` and `missingFields` (`{id, field}`) name the case to fix; `featuresWithoutSource`, `featuresWithoutCases`, `unknownCaseRefs` and `orphanCases` name broken Features rows or unmapped cases; `p0WithoutDiscriminates` names P0 cases without a discriminating fault; `structureErrors` names a section, table or column it could not find, which hides the checks that need it. Exit 2: wrong path or arguments. `--help` lists every field.

Judgment checks the script cannot make:

- The Features table lists every in-scope behavior; the script checks only the rows present.
- Actions are runnable at the pinned state: each command is pinned to its [source](references/code-map.md#layout-and-entry-points), never guessed; non-command steps define a checkable end state.
- Oracles are observable comparisons; relevant failures and recovery paths are covered.
- Each P0's Discriminates, including for an `existing` test, names a base revision, revert or seeded fault under which the case fails; a case that passes on both is coverage, not a check.
- Setup names variables without secrets; fixtures, dependencies and cleanup are explicit; cases do not duplicate the same risk at the same level.
- Success criteria cover P0, regressions and evidence; an executor with only the plan/repository can run case one.

After delivery, follow the [HTML offer](references/presentation.md#html-delivery). Use [report commands](references/report-output.md#render-and-retrieve) for accepted rendering or `--manifest` discovery with [render-report.mjs](scripts/render-report.mjs).
