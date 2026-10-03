# Description Tuning

Load when optimizing a skill's `description` — the primary trigger. Why: at startup agents see only `name` + `description`.

## Good descriptions

- User intent, not implementation internals.
- Pushy on scope: list contexts where the request does not name the domain.
- Not so broad that near-miss prompts activate it.
- Trigger-rich: list the intents agents use.
- Not redundant: no second `Triggers:` label; no "This skill applies when…"; no long quoted-synonym laundry lists; no CLI/schema/internals dump.

## Eval queries

- Positive trigger: vary phrasing, typos, explicitness, and complexity. Each query needs the skill; a one-step task the agent can do alone might not trigger any skill.
- Negative trigger: use near-misses that share keywords but need another skill.
- Train/validation split so edits don't overfit.
- Re-run when nondeterministic; compare trigger rates.

## Loop

1. Eval current description on train + validation.
2. Find missed triggers and false triggers.
3. Revise for the failure category, not exact keywords.
4. Strip rigid or redundant wording.
5. Pick best by validation pass rate.
6. Sanity-check with fresh unused queries.

Next: before calling done load `references/skill-review.md`; when scoring trigger fit load `references/quality.md`.
