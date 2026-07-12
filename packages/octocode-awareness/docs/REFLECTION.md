# Reflection And Self-Improvement

Reflection turns a verified outcome into reusable learning or owned follow-up. It is
not routine status and never self-authorizes source/instruction changes.

## Flow

```text
verified outcome -> reflect -> route -> human/user-approved apply -> verify -> close
```

```bash
octocode-awareness reflect record --agent-id "$OCTOCODE_AGENT_ID" \
  --workspace "$PWD" --task "<task>" --outcome worked \
  --lesson "<reusable result>" --compact
```

`--outcome` is required and accepts `worked`, `partial`, or `failed`. Failed outcomes
should carry a stable `--failure-signature` such as `test:work-overlap` or
`migration:v2-run-files`, not the full volatile error string.

## Route By Owner

| Need | Flag/output | Close when |
|---|---|---|
| Reusable verified lesson | `--lesson` memory | Rechecked later; superseded/forgotten when stale. |
| Remaining repo/code work | `--fix-repo` refinement | Applied, tested, same refinement marked done with a check receipt. |
| Harness/skill/tool gap | `--fix-harness` memory/proposal | Human approves, owning source changes, skill/tests pass. |
| Bad/missing instructions | `--fix-instructions` developer-review row | Author updates guidance, verifies, closes row. |
| Recurring eval failure | `--failure-signature` / eval JSON | Cluster cause fixed and verified. |

Use `--fix-file` for affected instruction/source paths. Keep one concern per row.
Relative fix files resolve against `--workspace`, even when the CLI is launched from
another directory.

Terminal closure is evidence-bearing:

```bash
octocode-awareness refinement set --refinement-id <id> --state done \
  --agent-id "$OCTOCODE_AGENT_ID" --check-receipt "<check and result>" --compact
```

The closer may differ from the reporting agent. Awareness appends the actor,
timestamp, and receipt to the refinement reasoning before it appears as Resolved.

## Weakness Mining

```bash
octocode-awareness reflect mine-weakness --workspace "$PWD" --compact
```

Mining groups repeated stable signatures. A cluster is evidence to inspect, not a
patch instruction. Fix one cause, verify it, then reflect with the same signature so
future review sees the result.

## Harness Proposals

```bash
octocode-awareness reflect export-harness --workspace "$PWD" --compact
octocode-awareness reflect developer-review --workspace "$PWD" --format markdown --compact
```

Exports are previews. They never patch `AGENTS.md`, `SKILL.md`, docs, hooks, or code.
Approved skill edits use Research + package tests + human review, then rebuild and a held-out check.

Harness self-edits also require the pre-edit guard:

- `OCTOCODE_ALLOW_HARNESS_APPLY=1`;
- safe non-main branch;
- explicit detached/non-repo override only when approved.

## Memory Vs Refinement Vs Signal

- Memory: verified future-useful fact/decision/gotcha.
- Refinement: owned work/handoff that must be completed.
- Signal: another participant must see/answer/act.
- Task: selectable durable implementation work under a plan.

Never use memory/refinement as a duplicate task queue. Never store secrets.

## Role Challenge

`reflect record --duo` returns temporary supporter/skeptic prompts; it does not spawn
an agent or store debate. Use a read-only subagent when independent source inspection
materially reduces risk. Agreement is not verification; capture only synthesis,
dissent, and a concrete check.

## Documentation Drift

```bash
octocode-awareness docs staleness \
  --targets-json '[{"docFile":"README.md","sourceDirs":["src"]}]' --compact
```

Staleness is a lead based on recorded edit activity. Check current source, update the
one owning doc, run contract/link checks, and regenerate projections only when needed.

A learning loop is closed only when its output has an owner, applied action, fresh
verification, and terminal row/projection state.
