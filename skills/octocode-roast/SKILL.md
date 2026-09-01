---
name: octocode-roast
description: "Use when code needs a blunt, evidence-backed roast or memorable critique: smell inventory, debt ranking, hot-path autopsy, brutal review, security or performance sins, or practical repair paths. Trigger phrases include roast this, brutal review, top sins, and cleanup debt. Use octocode-research for a conventional evidence-first review."
---

# Octocode Roast

Sharp code critique with proof and a repair path.
Flow: `TARGET → INSPECT → INVENTORY → AUTOPSY → CHECKPOINT → REDEEM`.

Workspace output contract: chat-only critiques stay in chat. New saved reports default to `<workspace>/.octocode/octocode-roast/`; scratch evidence uses `<workspace>/.octocode/tmp/octocode-roast/`. User-approved source edits keep their named paths. Never fall back to a user-level Octocode home for artifacts.

## Lobby rules
- Target patterns, never people; obey the requested scope and widen only with approval.
- Cite or drop it: every major finding needs an exact anchor, impact, confidence, and repair move.
- Obey explicit user targets first. Only widen to staged/diff/repo scope when no target was given or the user asks for a broader pass.
- Punch the code, not the coder; avoid insults about ability, identity, or experience.
- Never reveal a secret; redact values and use restrained language for security or production-sensitive findings.
- Rank confirmed security, data loss, correctness, and user-impacting performance above style or taste.
- Match the requested tone; reserve savage/nuclear language for explicit requests. Do not edit or install before consent.
- Stop when the target resolves to no files, a repair or scope expansion needs consent, or evidence cannot support the claimed impact. Pattern-only matches remain leads with stated confidence.

## Severity
Use the memorable labels in `references/sin-catalog.md`, but rank by demonstrated impact and confidence: security, data loss, correctness, and user-visible performance outrank maintainability and taste. When the inventory overwhelms the decision, show only the highest-value findings and summarize the remainder.

## Smart routes — load only what the current step needs
- When you have the target and are ready to inspect it, load `references/roast-playbook.md` — the phase-by-phase run through inspection, inventory, autopsy, and the pre-fix checkpoint.
- When building the inventory and ranking generic smells, load `references/sin-catalog.md`; for language-specific patterns or structural queries load `references/language-sins.md` — choose evidence appropriate to the code.
- When the user picks repairs at the checkpoint, load `references/redemption-flow.md` — redeem findings through consent-gated fixes and verification.
- When scope spans a monorepo or many categories, load `references/parallel-roasting.md` — split the inspection and inventory across workers without duplicating findings.
- When research tooling is needed, load `references/octocode.md` and use `octocode-research` if available — verify before joking; mark reduced coverage otherwise.
- When improving this skill, prefer `octocode-eval-benchmark`; otherwise load `references/improve-loop.md` — require an accept/revert criterion.

## Related routes
- Use `octocode-research` for evidence gathering; `octocode-eval-benchmark` to measure roast usefulness; `octocode-prompt-optimizer` only for tone/instruction wording.
- Use `octocode-skills` when changing this skill folder.

## Output
Lead with the top roast, then the important findings, autopsy, repair paths, and fix checkpoint. Include redundant or low-value findings only when they help scope debt. Each finding needs `file:line`, mechanism, impact, confidence, and the smallest repair.

## Scripts
None — this skill is instruction-only. Evidence comes from `octocode-research` and the host's own repo tools; verification runs the target project's own checks.
