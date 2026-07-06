# Description Tuning

Load when optimizing a skill's `description` — the primary trigger. At startup an agent sees only `name` and `description`, so the description must tell it when to load the skill.

## Good descriptions

- Use imperative phrasing: "Use this skill when..." / "Use when...".
- Focus on user intent, not implementation internals.
- Include non-obvious trigger situations where the user may not name the domain directly.
- Stay concise and under the 1024-character limit.
- Avoid being so broad that near-miss prompts trigger the skill.

## Test with eval queries

- Should-trigger prompts: vary phrasing, typos, explicitness, detail, and task complexity.
- Should-not-trigger prompts: near-misses that share keywords but need a different skill.
- Use train/validation splits so edits do not overfit the test prompts.
- Run multiple times when behavior is nondeterministic and compare trigger rates.

## Optimization loop

1. Evaluate the current description on train and validation sets.
2. Identify train failures: missed triggers and false triggers.
3. Revise for the general category of failure, not exact query keywords.
4. Keep the description under 1024 characters.
5. Select the best iteration by validation pass rate.
6. Sanity-check with fresh queries not used during optimization.
