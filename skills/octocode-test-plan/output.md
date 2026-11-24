# Output

<!-- report-contract {"statuses":["Ready","Draft","Blocked"],"required":["Answer","Scope","Executor contract","Features","Cases","Success criteria","Not tested","Gaps","Results format"],"anyOf":[]} -->

Write for an executor receiving only the plan and repository. Lead in chat with readiness, main risk and blocking gaps; link the saved plan. Cases are unexecuted.

## Saved report

Always save at `<workspace>/.octocode/octocode-test-plan/<YYYY-MM-DD>-<slug>.md`; scratch goes in its `tmp/`. `<workspace>` is the repository root, else current directory, never inside `.octocode/`. A user-named path wins.

Use the [shared header](references/report-output.md#report-header) and `question: "Test plan: <target>"`. Accepted `status` values: `Ready`, `Draft`, `Blocked`. Each means:

| Status | Meaning |
|---|---|
| `Ready` | Plan check passes; every P0 is runnable by the named executor |
| `Draft` | Named gaps remain, such as unconfirmed commands or unread code |
| `Blocked` | A missing decision, credential or environment prevents P0 execution; describe it as SBAR under Gaps |

Start `# Test plan: <target>` with the target, revision and meaning of success. Include the sections below in order; an empty section says `None: <why>`, without filler.

### Answer

Readiness, main risks and the gap most likely to change the result.

### Scope

Repository, revision (SHA or PR head), paths/features and source anchors. Identify relevant working-tree and untracked changes when they affect the planned result.

### Executor contract

Give the executor:

- **Objective/inputs:** run the cases at the pinned revision using this plan, repository and specified environment.
- **Tools/constraints:** required tools and permissions. Source/tests are read-only by default; never weaken, delete or silently skip a test. Report actual failures. Destructive or external steps need explicit scope.
- **Setup:** preparation commands, working directory, required environment variable names (never secrets) and confirmation checks. Make dependencies between steps explicit.
- **Revision check:** confirm the executor has the source state the plan describes; resolve relevant differences before using its expected results.
- **Starting state:** copy these commands into the plan. From the repository root, record the state before the first case and compare it after the last cleanup:

  ```sh
  tree_state() { git rev-parse HEAD; git status --porcelain=v1 --untracked-files=all -- . ':(exclude).octocode'; git diff HEAD --binary -- . ':(exclude).octocode' | git hash-object --stdin; git ls-files -z --others --exclude-standard -- . ':(exclude).octocode' | xargs -0 git hash-object --; }
  tree_state > <plan-dir>/<plan-name>-start-state.txt          # before the first case
  tree_state | diff <plan-dir>/<plan-name>-start-state.txt -   # after cleanup
  ```

  Exit 0 with no output means source and tests are unchanged; printed lines are the changes. When the plan is saved inside the repository but outside `.octocode/`, add `':(exclude)<plan-dir>'` to each pathspec.
- **Report/Done:** save `<plan-name>-results.md` beside the plan; every case has evidence and a result, and success criteria are evaluated.

### Features

Map every in-scope feature, documented claim or changed behavior:

| Feature or claim | Source | Risk | Priority | Cases |
|---|---|---|---|---|
| `<behavior>` | `<path:line or spec>` | `<what could fail and why>` | `P0/P1/P2` | `<case IDs or Not tested>` |

### Cases

Give each case a unique ID, title and fields below. Head each case `<ID> — <title>` one heading level below Cases; the ID is one token with a digit, such as `T1`, and other headings there are notes. Start each field on its own line as `Given: …`, `**Given:** …` or a `| Given | … |` row. A field written once in the Cases intro, before the first case, applies to every case. For a large plan, add a summary only when it helps navigation. Mark reused tests as existing; do not confuse coverage with a run result.

| Case field | Required content |
|---|---|
| Covers | Feature/claim and deciding source anchor |
| Priority/level | P0/P1/P2 and unit/integration/end-to-end/manual |
| Given | Setup state, fixtures and dependencies; pin time, randomness, locale, ordering and network, or specify repeat count |
| When | Action or ordered scenario; exact command/request/input where syntax or ordering matters. Otherwise state the target state and how to check it |
| Then | Observable comparison: value, property, pattern, status, file, count, or timing bound with unit/baseline/runs |
| Pass if / Fail if | Unambiguous oracle; derive expected behavior from a contract, not current output alone |
| Discriminates | For P0: the base revision, revert or seeded fault under which the case fails; the plan names it and the executor applies it, the plan edits nothing |
| Evidence to capture | Output, exit code, file or log needed to verify the result |
| Cleanup | Remove case-owned state and verify restoration, or say none |
| Skip if | Optional: the condition under which the executor may report `skipped`; without it, `skipped` is not allowed |

An `existing` case names its test's `path:line` and the command that selects it. Cases run independently unless Given names a dependency. Link repository setup rather than copying it; do not duplicate one risk at several levels unless they observe different boundaries.

### Success criteria

Set measurable run criteria. Default, adapt explicitly:

- Every P0 and existing regression case passes.
- Report failed or blocked cases with their impact; any acceptance exception needs an owner decision.
- Every case has evidence; missing evidence means `blocked`.
- Source/tests remain unchanged: the starting-state comparison exits 0 with no output.

Add existing coverage floors or performance bounds only when relevant, with baseline and units.

### Not tested

Each excluded feature, risk or environment, with its reason or a named coverage owner.

### Findings

Optional, before Gaps: defects encountered while mapping, with deciding anchors and a proposed proving case. Planning does not repair them.

### Gaps

Unread scope, unavailable semantic/graph evidence, unconfirmed commands, contradictory docs/code with both anchors, and labeled assumptions.

### Results format

| ID | Result | Evidence | Notes |
|---|---|---|---|
| `<ID>` | `pass/fail/blocked/skipped` | `<captured output/file/log>` | `<actual vs expected, or blocker>` |

Allow `skipped` only for a case with a `Skip if` field. Require an evaluated success-criteria checklist with reasons.

Before accepting a results file, yours under a separate execution request or an executor's, run from the workspace:

```sh
node <skill-folder>/scripts/plan-check.mjs <plan.md> --results <plan-name>-results.md
```

Its `results` object lists cases without a result (`missingResults`), rows naming no plan case (`unknownIds`), `duplicateResults`, values outside the four above (`invalidResults`), `skippedNotPermitted` and pass or fail rows without evidence (`withoutEvidence`). Exit 1 means the run is incomplete: report each finding with the results; a case without a valid result has not passed.

## Plan check

Before claiming `Ready`, run the [plan checker](scripts/plan-check.mjs) on the saved plan from the workspace, then make the judgment checks below. Fix each failure or list it under Gaps and use `Draft` (or `Blocked` for a P0 prerequisite).

```sh
node <skill-folder>/scripts/plan-check.mjs <plan.md>
```

Exit 0 (`ok: true`): the mechanical checks pass. Exit 1: every non-empty list is a failure. `duplicateIds` and `missingFields` (`{id, field}`) name the case to fix; `featuresWithoutSource`, `featuresWithoutCases`, `unknownCaseRefs` and `orphanCases` name broken Features rows or unmapped cases; `p0WithoutDiscriminates` names P0 cases without a discriminating fault; `structureErrors` names a section, table or column it could not find, which hides the checks that need it. Exit 2: wrong path or arguments. `--help` lists every field.

Judgment checks the script cannot make:

- The Features table lists every in-scope behavior; the script checks only the rows present.
- Actions are runnable at the pinned state: commands come from manifests/CI/docs or authorized inspection; non-command steps define a checkable end state.
- Oracles are observable comparisons; relevant failures and recovery paths are covered.
- Each P0's Discriminates, including for an `existing` test, names a base revision, revert or seeded fault under which the case fails; a case that passes on both is coverage, not a check.
- Setup names variables without secrets; fixtures, dependencies and cleanup are explicit; cases do not duplicate the same risk at the same level.
- Success criteria cover P0, regressions and evidence; an executor with only the plan/repository can run case one.

After delivery, follow the [HTML offer](references/presentation.md#html-delivery). Use [report commands](references/report-output.md#render-and-retrieve) for accepted rendering or `--manifest` discovery with [render-report.mjs](scripts/render-report.mjs).
