# Routing
Load when a loop produces a durable lesson or needs another Octocode skill. Why: one owner per concept.

| Need | Route |
|---|---|
| Skill folder create/edit/review | `octocode-skills` (+ `skill-review.mjs`) |
| Code/GitHub/package evidence | `octocode-research` |
| RFC success metrics document | `octocode-rfc-generator` → `KPI.md` |
| Prompt/trigger tuning only | `octocode-prompt-optimizer` (still run this skill’s KPI loop) |
| Idea worth-building before eval suite | `octocode-brainstorming` |
| Parallel workers / model routing | `octocode-subagent` |
| Measure/manage subagents (KPI, protocol, comms) | `references/subagent-cookbook.md` → protocol/KPIs/communication/approaches |
| Blunt code critique | `octocode-roast` |

## Capture rules
- Record only **verified** reusable outcomes
- Attach `failureSignature` for recurring fails (`mechanism:…|cause:…`)

## This skill owns
Eval contract, KPI framing, keep/discard loop, grader choice, held-out discipline, and subagent **measurement** cookbooks (not spawn APIs).

Next: improve cycle checklist → `references/improve-loop.md`; subagent measurement → `references/subagent-cookbook.md`.
