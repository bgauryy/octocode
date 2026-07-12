# Learning Loop Closure

Use this when reflection, evals, recurring failures, developer review, or harness proposals should change future behavior.
Learning-loop work is **bookkeeping (learning)**. Consult `references/bookkeeping.md` only when cleanup/trigger policy is still needed and not already loaded. Skip routine successful edits with no reusable lesson.

A loop is closed only when its output has an owner, an applied action, fresh verification, and a terminal state or refreshed projection.

## Routes

| Trigger | Produce | Consume | Close |
|---|---|---|---|
| Reusable outcome | `reflect record --lesson` | later `attend` / `memory recall` | Re-check; supersede/forget when stale. |
| Repo/code fix | `--fix-repo` refinement | `refinement get --state open` | Apply, verify, then close with agent and check receipt. |
| Harness gap | `--fix-harness` memory | `reflect export-harness` | Human applies; skill review/tests; re-reflect. |
| Bad instructions | `--fix-instructions` | `reflect developer-review` | Update instructions; mark done; confirm the live view. |
| Repeated failure | `--failure-signature` / `--eval-failure-json` | `reflect mine-weakness` | One cluster → one fix → re-reflect same signature. |
| Role prompts | `reflect record --duo` | one internal dialogue | Capture synthesis only. |
| Independent challenge | rubber-duck subagent | main revises + next check | Never treat agreement as proof. |
| Stale docs | `docs staleness` | source owner | Update + regenerate needed projections. |
| Cleanup pressure | digest/prune/forget dry-runs | reviewed IDs | Mutate, then re-`attend`/`query`. |

Terminal recipe: `refinement set --refinement-id <id> --agent-id "$OCTOCODE_AGENT_ID" --state done --check-receipt "<check and result>"`.

## Failures

Capture so errors cluster: `reflect record --outcome failed --failure-signature "<stable key>" --lesson "…"`. Stable key = `test:<name>` or `<class>:<site>`, not the full message. Bulk: `--eval-failure-json '[...]'`. Mine with `reflect mine-weakness`; route `--fix-repo|harness|instructions`; re-reflect with the **same** signature. `--outcome` must be `worked|partial|failed`.

## Durable output (after `wiki sync`)

| Write with | Lands in |
|---|---|
| Verified reusable knowledge, gotchas, lessons, or external references | bounded `.octocode/KNOWLEDGE.md` when nonempty |
| Local `--reference file:…` / `--file` paths | live `query files`; projection only when the selected knowledge lead includes them |
| `--fix-instructions` feedback | live `reflect developer-review` / `query developer-review`; not projected by default |
| Discovery and command routing | lean `.octocode/AGENTS.md` |
| Projection ownership/completeness | `.octocode/awareness/manifest.json` |

SQLite is canonical; sync only when file readers need refresh. Current projection ownership: `references/wiki-files-map.md`.

## Sequence

```text
VERIFIED OUTCOME -> REFLECT -> ROUTE -> APPLY -> VERIFY -> CLOSE ROW -> PROJECT IF USEFUL -> ATTEND
```

Use `--duo` for hard judgments; `subagent-rubber-duck.md` for a real second agent. `none` closes when nothing durable remains. `export-harness` is preview-only; `wiki sync` publishes DB state separately. Keep memory/refinement IDs until closure.
