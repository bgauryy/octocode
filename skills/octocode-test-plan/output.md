# Output

<!-- report-contract {"statuses":["Ready","Draft","Blocked"],"required":["Answer","Scope","Executor contract","Features","Cases","Success criteria","Not tested","Gaps","Results format"],"anyOf":[]} -->

The plan is the deliverable. Write it for an executor that has not seen this conversation, the code map or the sources you read: it gets the plan file and the repository, nothing else.

## Response

Lead with the status, the number of cases by priority and the main risk the plan covers. Link the saved plan. List open gaps that block a `Ready` status.

## Saved report

Always save the plan. Path: `<workspace>/.octocode/octocode-test-plan/<YYYY-MM-DD>-<slug>.md`, where `<slug>` is the target in kebab case; scratch notes go in its `tmp/`. `<workspace>` is the repository root, else the current directory, never a folder inside `.octocode/`. A user-named path wins.

Use the [shared header](references/report-output.md#report-header). `question` is `"Test plan: <target>"`. Accepted `status` values: `Ready`, `Draft`, `Blocked`.

- `Ready`: the plan check passes and every P0 case is runnable by the named executor.
- `Draft`: the plan check passes except named gaps, such as an unconfirmed command or an unread module.
- `Blocked`: a missing decision, credential or environment stops P0 cases; name it as SBAR under `## Gaps`.

Title it `# Test plan: <target>`, then a lead paragraph with what is tested, at which revision and what success means. Include these `##` sections, in this order. Write only what this target needs: a section with nothing to say is one line (`None: <why>`), never template filler.

### Answer

Status, case counts by priority and level, the main risks covered and the one gap most likely to change the result.

### Scope

Repository, revision (commit SHA, or the PR head; on a dirty tree also the diff hash `git diff HEAD | git hash-object --stdin`), the in-scope paths and features, and the sources read to build the plan, as `path:line`.

### Executor contract

The executor's Task contract, written to it:

- **Objective:** run the cases below against `<revision>` and report each result.
- **Inputs:** this plan, the repository at the pinned revision, the environment and setup below.
- **Tools:** what the executor needs (shell, browser, Octocode, network) and which commands or tools each case uses.
- **Constraints:** what the executor may and may not change. Default: treat source and tests as read-only; never edit, delete or skip a test or weaken an assertion to make a case pass: report `fail` with the actual result instead. Never skip a case silently, and run destructive or external steps only where the case says so.
- **Setup:** ordered commands to prepare the environment (install, build, seed data, environment variables by name, never values), each with how to confirm it worked. Every command runs from the repository root in a fresh shell; a case that needs a variable or directory sets it in its own command.
- **Check-back:** before running, confirm `git rev-parse HEAD`, and on a dirty tree the diff hash, match the pinned revision (a difference stops the run and is reported), then restate the objective and the success criteria in one paragraph.
- **Report:** the table under Results format, saved next to the plan as `<plan-name>-results.md`.
- **Definition of Done:** every case has a result with evidence, and the success criteria are evaluated.

### Features

Traceability table, one row per feature, documented claim or changed behavior:

| Feature or claim | Source | Risk | Priority | Cases |
|---|---|---|---|---|
| `--json` prints the envelope | `src/cli.ts:88`, help text `src/help.ts:40` | Public contract, changed in this diff | `P0` | `TC-03`, `TC-04` |

Every row lists at least one case or points to `## Not tested`.

### Cases

A summary table, then one block per case.

| ID | Title | Priority | Level | Technique | Status |
|---|---|---|---|---|---|
| `TC-03` | `--json` on an empty result exits 1 with an empty `results` row | `P0` | integration | boundary | new |

`Status` is `new` or `existing` (a current test already covers it). Each case block:

```markdown
#### TC-03 `--json` on an empty result exits 1 with an empty `results` row

- **Covers:** Features row 2; `src/cli.ts:88`
- **Priority / level:** P0, integration
- **Given:** the CLI is built (`yarn build`), and `fixtures/empty/` holds no matching file
- **When:** `node out/cli.js search '{"queries":[{"path":"fixtures/empty","matchString":["x"]}]}' --json; echo "exit=$?"`
- **Then:** stdout parses as JSON; `results[0].status` is `"empty"`; the last line is `exit=1`
- **Pass if:** all three hold. **Fail if:** any differs; record the actual value.
- **Evidence to capture:** full stdout and the exit line
- **Cleanup:** none
```

Rules for each case:

- Match detail to fragility: give the exact command, request or input where the result depends on it or steps must run in order; where many paths reach the same state, state that state and how to check it. Link repository files instead of copying what they already say.
- **Given** is a state the executor can create or check; **When** is one action; **Then** is an observable result checked by comparison, not by opinion: an exit code, an exact or pattern-matched output, a file's content, a status code, a row count or a timing bound with its unit.
- An `existing` case gives the command that runs only that test (a filter or test name) and the test's `path:line`.
- A case that depends on another case names it under **Given**. Cases otherwise run in any order.
- Flaky inputs (time, randomness, network, ordering) are pinned in **Given** or the case says how many runs decide it.

### Success criteria

Plan-level exit criteria the executor evaluates after the run. Each is measurable. Default set, adjust to the target:

- [ ] Every `P0` case passes.
- [ ] No `P1` case fails without a filed reason; at most the stated number are `blocked`, each with its cause.
- [ ] Every `existing` case passes at the pinned revision, so nothing regressed.
- [ ] Every case has captured evidence; a result without evidence counts as `blocked`.
- [ ] No source or test file changed during the run (`git status --porcelain` matches the start).

Name any extra criterion the target needs, such as a coverage floor already enforced by the repository or a performance bound with its baseline.

### Not tested

Features, risks or environments deliberately left out, each with why: out of scope, covered elsewhere (name it), needs an unavailable environment, or low risk.

### Findings

Optional, before Gaps: defects seen while mapping, such as documentation the code contradicts or a path that cannot work. Each gives both anchors and the case that will show it, if any. The plan does not fix them.

### Gaps

What the planner could not verify: unread scope, unavailable LSP or graph, commands not confirmed, documentation that contradicts code (both anchors), and assumptions marked as assumptions.

### Results format

Give the executor this table to fill:

| ID | Result | Evidence | Notes |
|---|---|---|---|
| `TC-03` | `pass` \| `fail` \| `blocked` \| `skipped` | Output excerpt, file or log path | Actual vs expected on fail; cause on blocked |

`skipped` only when the plan says the case may be skipped. Then the success criteria with each box checked or not and why.

## Plan check

Run before saving with status `Ready`. Fix each failure or record it under `## Gaps` and use `Draft`.

- [ ] Every in-scope feature, documented claim and changed behavior appears in `## Features` with a source anchor.
- [ ] Every Features row maps to a case or to `## Not tested`.
- [ ] Every case has Given, When, Then, Pass if, Evidence to capture and a priority.
- [ ] Every **When** is runnable at the pinned revision: each command was read from manifests, CI or docs, or confirmed with a list or dry-run mode; a step without a command states the end state and how to check it.
- [ ] Every **Then** is observable and checked by comparison; no "works correctly", "looks right" or "as expected".
- [ ] Each P0 has a negative or error-path case beside its happy path.
- [ ] Setup names environment variables, never secrets.
- [ ] Case IDs are unique; no two cases test the same thing at the same level.
- [ ] Success criteria are measurable and cover P0, regressions and evidence.
- [ ] A reader with only the plan and the repository could run case one.

After delivery, follow the [HTML offer](references/presentation.md#html-delivery). Use [report commands](references/report-output.md#render-and-retrieve) for accepted rendering and `--manifest` discovery; the renderer is [render-report.mjs](scripts/render-report.mjs).
