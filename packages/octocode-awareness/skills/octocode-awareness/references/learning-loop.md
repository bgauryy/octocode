# Learning Loop Closure

Use this when reflection, evals, recurring failures, developer review, or harness proposals should change future behavior. This is **bookkeeping (learning)**; read `references/bookkeeping.md` for cleanup and triggers. Skip routine successful edits with no reusable lesson.

A loop is closed only when its output has an owner, an applied action, fresh verification, and a terminal state or refreshed projection.

## Routes

| Trigger | Produce | Consume | Close |
|---|---|---|---|
| Reusable outcome | `reflect record --lesson` | later `attend` / `memory recall` | Re-check; supersede/forget when stale. |
| Repo/code fix | `--fix-repo` refinement | `refinement get --state open` | Apply, verify, then close with agent and check receipt. |
| Harness gap | `--fix-harness` memory | `reflect export-harness` | Human applies; skill review/tests; re-reflect. |
| Bad instructions | `--fix-instructions` | `reflect developer-review` | Update instructions; mark done; optional inject. |
| Repeated failure | `--failure-signature` / `--eval-failure-json` | `reflect mine-weakness` | One cluster → one fix → re-reflect same signature. |
| Role prompts | `reflect record --duo` | one internal dialogue | Capture synthesis only. |
| Independent challenge | rubber-duck subagent | main revises + next check | Never treat agreement as proof. |
| Stale docs | `docs staleness` | source owner | Update + regenerate needed projections. |
| Cleanup pressure | digest/prune/forget dry-runs | reviewed IDs | Mutate, then re-`attend`/`query`. |

Terminal recipe: `refinement set --refinement-id <id> --agent-id "$OCTOCODE_AGENT_ID" --state done --check-receipt "<check and result>"`.

## Failures

Capture so errors cluster: `reflect record --outcome failed --failure-signature "<stable key>" --lesson "…"`. Stable key = `test:<name>` or `<class>:<site>`, not the full message. Bulk: `--eval-failure-json '[...]'`. Mine with `reflect mine-weakness`; route `--fix-repo|harness|instructions`; re-reflect with the **same** signature. `--outcome` must be `worked|partial|failed`.

## Label → wiki (after `repo inject`)

| Write with | Lands in |
|---|---|
| `--label GOTCHA` or `--failure-signature` | `.octocode/GOTCHAS.md` |
| `--lesson` / `DECISION`/`ARCHITECTURE`/`WORKFLOW`/… | `.octocode/LEARN.md` |
| any durable memory | `.octocode/MEMORY.md` |
| `--reference` / `--file` | `.octocode/BOOKMARKS.md` |
| `--fix-instructions` | `.octocode/DEVELOPER_REVIEW.md` |
| digest of the above | `.octocode/AGENTS.md` |

SQLite is canonical; inject only when file readers need refresh.

## Sequence

```text
VERIFIED OUTCOME -> REFLECT -> ROUTE -> APPLY -> VERIFY -> CLOSE ROW -> PROJECT IF USEFUL -> ATTEND
```

Use `--duo` for hard judgments; `subagent-rubber-duck.md` for a real second agent. `none` closes when nothing durable remains. `export-harness` is preview-only; `repo inject` publishes DB state separately. Keep memory/refinement IDs until closure.
