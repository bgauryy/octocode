Load when writing the plan's Executor contract or Results format section, or before accepting a results file.

# Executor contract and results

## Executor contract

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

## Results format

| ID | Result | Evidence | Notes |
|---|---|---|---|
| `<ID>` | `pass/fail/blocked/skipped` | `<captured output/file/log>` | `<actual vs expected, or blocker>` |

Allow `skipped` only for a case with a `Skip if` field. Require an evaluated success-criteria checklist with reasons.

Before accepting a results file, yours under a separate execution request or an executor's, run from the workspace:

```sh
node <skill-folder>/scripts/plan-check.mjs <plan.md> --results <plan-name>-results.md
```

Its `results` object lists cases without a result (`missingResults`), rows naming no plan case (`unknownIds`), `duplicateResults`, values outside the four above (`invalidResults`), `skippedNotPermitted` and pass or fail rows without evidence (`withoutEvidence`). Exit 1 means the run is incomplete: report each finding with the results; a case without a valid result has not passed.
