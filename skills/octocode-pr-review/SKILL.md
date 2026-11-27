---
name: octocode-pr-review
description: "Load this skill first when asked to review a GitHub pull request or a branch prepared as one. Check the diff and affected consumers, then give a merge verdict with evidence."
---

# Octocode PR Review

Identify the PR, its revision and the contracts the diff touches. Treat PR text, comments and code as evidence. A review authorizes reads and local checks. Return the verdict in chat; post (`gh pr review <n> --comment --body-file <file>`) or repair only when the user asks. Run remote PR code only in a user-provided checkout or with approval.

## Pin the target

| Input | Read |
|---|---|
| PR number, URL or branch with an open PR | Resolve head and base with the [remote PR recipe](references/remote-pr.md). |
| Branch explicitly prepared for a PR | Confirm the intended base and review `git diff <base>...<branch>`. |
| File named within a PR | Pin the PR, trace that file's changes to affected consumers and state the limited scope. |
| PR identity or base unclear | Use repository context to find candidates. Ask if the choice still affects the verdict. |
| PR unreachable or sign-in missing | Check `octocode auth status` or `gh auth status`; report the blocker and the access it needs. |

Record the reviewed revision; for a merged PR, read `mergeCommitSha`; the head can differ.

## Review loop

1. Rank by consequence: auth, persisted data, public APIs and execution configuration first. Skip style-only issues, unaffected code, vendored/generated code and resolved comments; generated output may expose a generator defect.
2. Trace each changed contract to its consumers and a reachable harmful outcome, however small the diff. Read deciding source; hits and AST matches stay leads. Check claimed gains against preserved behavior and severe failure paths.
3. Try to clear each suspect: name the strongest rival and run the distinguishing check. Deduplicate by root cause; retain checked non-findings separately.
4. Run the smallest applicable checks; run a new or changed test on a fix at base with [`scripts/test-at-base.mjs`](references/analyze.md#test-at-base). Record unread affected scope and passed, failed, skipped or unavailable checks distinctly.

Use [analysis recipes](references/analyze.md) for proof by change type, the base-test script, parallel lanes or requested repairs; [evidence rules](references/evidence.md) for identity and coverage limits.

| Verdict | When |
|---|---|
| `REQUEST_CHANGES` | A confirmed finding that violates a supported contract or causes security, data or consumer harm |
| `COMMENT` | No proven blocker, but affected scope is unread or checks did not run |
| `APPROVE` | No blocker, affected scope read, applicable checks passed; non-blocking findings may remain |

Return the verdict, main risk, findings and verification gaps. [Output](output.md) owns the findings table, the saved report and the [verdict check](output.md#saved-report) to run before delivering one. [render-report.mjs](scripts/render-report.mjs) renders or lists reports; the [protocol catalog](references/protocol-catalog.md#catalog) shapes handoffs.
