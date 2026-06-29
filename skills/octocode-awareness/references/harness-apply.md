# Harness Apply

Read this only when the user explicitly asks to edit `octocode-awareness` itself:
prompts, hooks, checks, `awareness.py`, schemas, or references. Normal task use should
record a proposed harness fix with `reflect --fix-harness`; this file is for applying
a reflected harness fix after a human opens the gate.

## Gate

An agent may edit the skill itself only through this gated path:

1. **Human approval:** a person exports `OCTOCODE_ALLOW_HARNESS_APPLY=1` for the
   session.
2. **Branch isolation:** the skill repo is on a branch that is not `main`/`master`
   unless `OCTOCODE_HARNESS_BRANCH_OK=1` is deliberately set.
3. **Audit + broadcast:** run:

```bash
harness-apply --agent-id <a> --approved-by <human> --change "<summary>" --file <skill file...>
```

The `harness-apply` command records a `HARNESS_APPLY` audit event, broadcasts a `decision` notification, and
returns a human-facing message to surface.

4. **Edit, verify, review:** make the change, run the declared verification, release
   locks with verification, and leave a diff/PR for human review.

## Guard Behavior

The `harness-guard` PreToolUse hook blocks `Write`/`Edit` to files inside this skill
directory unless the gate and branch checks pass. It uses the same coordination path
as `pre-edit.sh`, but turns it onto the harness itself.

## Never

- Never silently self-modify this skill.
- Never auto-merge harness edits.
- Never turn failed eval questions directly into prompt bloat; collapse duplicates
  and separate missing tooling from missing instruction.
