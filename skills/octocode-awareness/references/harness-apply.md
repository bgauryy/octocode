# Harness Apply

Read this only when the user explicitly asks to apply a harness-surface change:
`AGENTS.md`/`CLAUDE.md`, docs, standing memory-corpus changes, or `octocode-awareness`
prompts, hooks, checks, scripts, schemas, or references. Normal task use records a
proposal; this file is for applying it after a human opens the gate.

## Gate

An agent may apply a harness-surface change only through this gated path:

1. **Fix request first:** before asking for approval, name the target files, why the
   change is needed, the evidence, proposed edit, risk/rollback, and verification
   plan. Do this for `AGENTS.md`, docs, standing memory-corpus changes, and skill-code changes alike.
2. **Human approval:** a person explicitly approves that scoped change; hook-enforced
   sessions also need `OCTOCODE_ALLOW_HARNESS_APPLY=1`.
3. **Branch isolation:** the skill repo is on a branch that is not `main`/`master`
   unless `OCTOCODE_HARNESS_BRANCH_OK=1` is deliberately set.
4. **Audit + broadcast:** run:

```bash
harness-apply --agent-id <a> --approved-by <human> --change "<summary>" \
  --why-needed "<future failure/decision changed>" --evidence "<source>" \
  --risk "<risk + rollback>" --verification-plan "<checks>" --file <skill file...>
```

The extra reason fields preserve the fix request in the audit trail. The `harness-apply` command records a `HARNESS_APPLY` event, broadcasts a `decision` notification, and returns a human-facing message to surface.

5. **Edit, verify, review:** make the change, run the declared verification, release
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
