# Description Tuning

Load when optimizing a skill's `description` — the primary trigger. Why: at startup agents see only `name` + `description`.

## Good descriptions

- Imperative: "Use when…".
- User intent, not implementation internals.
- Non-obvious triggers where the user may not name the domain.
- Concise, ≤1024 chars; first ~50 chars state the trigger.
- Not so broad that near-miss prompts activate it.
- **Trigger-rich, not rigid:** list intents agents will say; keep exclusivity/mandate language out of the description — hard rules belong in the lobby body.
- **Not redundant:** one `Use when` clause; no second `Triggers:` label; no "This skill should be used when…"; no long quoted-synonym laundry lists; no CLI/schema/internals dump.

## Eval queries

- Should-trigger: vary phrasing, typos, explicitness, complexity.
- Should-not-trigger: near-misses sharing keywords but needing another skill.
- Train/validation split so edits don't overfit.
- Re-run when nondeterministic; compare trigger rates.

## Loop

1. Eval current description on train + validation.
2. Find missed triggers and false triggers.
3. Revise for the failure category, not exact keywords.
4. Stay ≤1024 chars; strip rigid/redundant wording.
5. Pick best by validation pass rate.
6. Sanity-check with fresh unused queries.

Next: before calling done load `references/skill-review.md`; when scoring trigger fit load `references/quality-rubric.md`.
