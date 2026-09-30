---
name: octocode-roast
description: "Use when a blunt evidence-backed code roast is wanted: rank smells, debt, hot paths, top sins, and cleanup priorities."
---

# Octocode Roast

tools: `npx octocode` / `octocode-mcp`
related-skill: `octocode-clean-agentic-code`
output: `<workspace>/.octocode/` for workspace work | `<home>/.octocode/` when no workspace applies
routes: load/run a reference, doc, or script only when it changes the next action; otherwise keep the rule here.

Critique code sharply; prove each finding and give a repair path.
Flow: `TARGET → INSPECT → INVENTORY → AUTOPSY → CHECKPOINT → REDEEM`.

Reports: `<output>/octocode-roast/`; scratch: `<output>/tmp/octocode-roast/`. Chat-only critiques stay in chat; approved source edits keep their named paths.

## Lobby rules
- Target patterns, never people; obey the requested scope and widen only with approval.
- Cite or drop it: every major finding needs an exact anchor, impact, confidence, and repair move.
- Use explicit user targets first. Widen to staged/diff/repo scope only when no target exists or you approve a broader pass.
- Punch the code, not the coder; avoid insults about ability, identity, or experience.
- Never reveal a secret; redact values and use restrained language for security or production-sensitive findings.
- Rank confirmed security, data loss, correctness, and user-impacting performance above style or taste.
- Match the requested tone; reserve savage/nuclear language for explicit requests. Do not edit or install before consent.
- Stop when the target resolves to no files, a repair, or scope expansion needs consent, or evidence cannot support the claimed impact. Pattern-only matches remain leads with stated confidence.

## Severity
Use the memorable labels in `references/sin-catalog.md`, but rank by demonstrated impact and confidence: security, data loss, correctness, and user-visible performance outrank maintainability and taste. When the inventory overwhelms the decision, show only the highest-value findings and summarize the remainder.

## Smart routes — load only what the current step needs
- Once the target is clear, load `references/roast-playbook.md` — the phase-by-phase run through inspection, inventory, autopsy, and the pre-fix checkpoint.
- When building the inventory and ranking generic smells, load `references/sin-catalog.md`; for language-specific patterns or structural queries load `references/language-sins.md` — choose evidence appropriate to the code.
- When you pick repairs at the checkpoint, load `references/redemption-flow.md` — redeem findings through consent-gated fixes and verification.
- When scope spans a monorepo or many categories, load `references/parallel-roasting.md` — split the inspection and inventory across workers without duplicating findings.
- For research tooling, use `octocode-research`; if unavailable, use `octocode-mcp` / `npx octocode` and mark reduced coverage.
- When improving this skill, use `octocode-eval-benchmark`.

## Related routes
- Use `octocode-research` for evidence, `octocode-eval-benchmark` to measure usefulness, `octocode-prompt-optimizer` for wording only.
- Use `octocode-skills` when changing this skill folder.

## Output
Lead with the top roast, then the important findings, autopsy, repair paths, and fix checkpoint. Include redundant or low-value findings only when they help scope debt. Each finding needs `file:line`, mechanism, impact, confidence, and the smallest repair.

## Scripts
None — this skill is instruction-only. Evidence comes from `octocode-research` and the host's own repo tools; verification runs the target project's own checks.
