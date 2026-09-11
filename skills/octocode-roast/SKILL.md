---
name: octocode-roast
description: "Use when a blunt evidence-backed code roast is wanted: rank smells, debt, hot paths, top sins, and cleanup priorities."
---

# Octocode Roast

tools: `npx octocode` / `octocode-mcp`
related-skill: `octocode-clean-agentic-code`
output: `<workspace>/.octocode/` for workspace work | `<home>/.octocode/` when no workspace applies
routes: load/run a reference, doc, or script only when it changes the next action; otherwise keep the rule here.

Sharp code critique with proof and a repair path.
Flow: `TARGET → INSPECT → INVENTORY → AUTOPSY → CHECKPOINT → REDEEM`.

Reports: `<output>/octocode-roast/`; scratch: `<output>/tmp/octocode-roast/`. Chat-only critiques stay in chat; approved source edits keep their named paths.

## Lobby rules
- Target patterns, never people; obey the requested scope and widen only with approval.
- Cite or drop it: every major finding needs an exact anchor, impact, confidence, and repair move.
- Obey explicit user targets first. Only widen to staged/diff/repository scope when no target was given or you ask for a broader pass. <!-- style-lint: ignore-line passive-voice -->
- Punch the code, not the coder; avoid insults about ability, identity, or experience.
- Never reveal a secret; redact values and use restrained language for security or production-sensitive findings.
- Rank confirmed security, data loss, correctness, and user-impacting performance above style or taste.
- Match the requested tone; reserve savage/nuclear language for explicit requests. Do not edit or install before consent.
- Stop when the target resolves to no files, a repair, or scope expansion needs consent, or evidence cannot support the claimed impact. Pattern-only matches remain leads with stated confidence.

## Severity
Use the memorable labels in `references/sin-catalog.md`, but rank by demonstrated impact and confidence: security, data loss, correctness, and user-visible performance outrank maintainability and taste. When the inventory overwhelms the decision, show only the highest-value findings and summarize the remainder.

## Smart routes — load only what the current step needs
- When you have the target and are ready to inspect it, load `references/roast-playbook.md` — the phase-by-phase run through inspection, inventory, autopsy, and the pre-fix checkpoint.
- When building the inventory and ranking generic smells, load `references/sin-catalog.md`; for language-specific patterns or structural queries load `references/language-sins.md` — choose evidence appropriate to the code.
- When you pick repairs at the checkpoint, load `references/redemption-flow.md` — redeem findings through consent-gated fixes and verification.
- When scope spans a monorepo or many categories, load `references/parallel-roasting.md` — split the inspection and inventory across workers without duplicating findings.
- When research tooling is needed, use `octocode-research`; if unavailable, use `octocode-mcp` or `npx octocode`, inspect live context/schema once, follow executable continuations, and mark reduced coverage — verify before joking. <!-- style-lint: ignore-line passive-voice -->
- When improving this skill, use `octocode-eval-benchmark`; if unavailable, record goal, KPI, baseline, smallest change, comparable result, and accept/revert.

## Related routes
- Use `octocode-research` for evidence gathering; it owns the MCP/CLI workflow and live tool/grammar discovery. Use `octocode-eval-benchmark` to measure roast usefulness and `octocode-prompt-optimizer` only for tone/instruction wording.
- Use `octocode-skills` when changing this skill folder.

## Output
Lead with the top roast, then the important findings, autopsy, repair paths, and fix checkpoint. Include redundant or low-value findings only when they help scope debt. Each finding needs `file:line`, mechanism, impact, confidence, and the smallest repair.

## Scripts
None — this skill is instruction-only. Evidence comes from `octocode-research` and the host's own repository tools; verification runs the target project's own checks.
