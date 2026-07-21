# Skill Scripts

Load when adding or reviewing `scripts/` — prefer code over agentic prose for mechanical steps.

## Why scripts

A script is more reliable, token-cheap, and identical every run.
Reserve prose for judgment; hand procedure to `scripts/`.
Use one-off shell only when an existing tool already does the job; pin versions when needed.

## Agent-facing contract

- Input via flags, env, files, or stdin — never interactive prompts.
- Concise `--help` with examples.
- Errors say what failed, what was expected, what to try.
- Structured data on stdout; diagnostics on stderr.
- Idempotent or safe to retry; reject ambiguous input.
- `--dry-run` for destructive/stateful ops.
- Meaningful exit codes; bounded or paginated output.
- Reference from `SKILL.md` as `scripts/skill-review.mjs` (or the real script name) with when/why.

## When to extract

Move complex or repeatedly reinvented logic into `scripts/`. If `SKILL.md` has a long numbered command-like procedure with no helper, extract it — the review flags `deterministic-prose`.

Next: when verifying routing load `references/skill-review.md`; if the script is a hook brain load `references/hooks-add.md`.
