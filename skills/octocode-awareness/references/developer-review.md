# Developer Review

Use this when human-authored instructions caused time loss, guessing, or a wrong turn. General reflection closure lives in `references/learning-loop.md`.

## Choose The Target

| Flag | Fixes | Output/consumer |
|---|---|---|
| `--fix-repo` | Code/repo behavior | Coding refinement for the next agent. |
| `--fix-harness` | Skill/tool/hook machinery | Harness memory for `reflect export-harness`; human applies. |
| `--fix-instructions` | AGENTS/SKILL/system/task wording | Instruction memory + developer-review refinement for the author. |

Use instruction feedback when missing values force guesses, rules worsen work, or wording needs repeated interpretation.
Name the source, cost, and proposed replacement; attach instruction files with `--fix-file`. Keep one concern per call.

```bash
octocode-awareness reflect record --agent-id "$OCTOCODE_AGENT_ID" \
  --workspace "$PWD" --task "add lock retry" --outcome partial \
  --fix-instructions "AGENTS.md omits the lock TTL; document the limit and extension path." \
  --fix-file AGENTS.md --compact
```

One call writes a tagged durable memory and an `instructions` refinement. The coding refinement queue hides instruction rows; workboard and developer-review views expose them.

## Consume And Close

- Human-readable: `reflect developer-review --format markdown`.
- Script/API: `query developer-review` JSON.
- Workboard: `DeveloperReview` column for open feedback.
- Published snapshot: `.octocode/DEVELOPER_REVIEW.md` after `wiki sync`; never hand-edit.

After updating the owning instruction, verify the changed behavior. Close the same row with `refinement set --refinement-id <id> --agent-id "$OCTOCODE_AGENT_ID" --state done --check-receipt "<check and result>"`. Inject only if file readers need refreshed Open/Resolved output.

Package maintainers: use this channel if Awareness's own instructions misled the agent. Strong, closed feedback should reduce repeat items.
