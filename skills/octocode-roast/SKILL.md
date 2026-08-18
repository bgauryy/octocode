---
name: octocode-roast
description: "Use when code needs a blunt evidence-backed roast or memorable critique: smell inventory, debt ranking, hot-path autopsy, savage/diff review, security or performance sins, or practical redemption paths. Phrases like roast this, brutal review, top sins, cleanup debt. Polite evidence-first PR review → octocode-research."
---

# Octocode Roast

Sharp code critique with proof and a repair path.
Flow: `TARGET → INSPECT → INVENTORY → AUTOPSY → CHECKPOINT → REDEEM`.

## Lobby rules
- Target patterns, never people; obey the requested scope and widen only with approval.
- Cite or drop it: every major finding needs an exact anchor, impact, confidence, and repair move.
- Obey explicit user targets first. Only widen to staged/diff/repo scope when no target was given or the user asks for a broader pass.
- Punch the code, not the coder; avoid insults about ability, identity, or experience.
- Never reveal a secret; redact values and use restrained language for security or production-sensitive findings.
- Rank confirmed security, data loss, correctness, and user-impacting performance above style or taste.
- Default to medium tone; use savage/nuclear only when explicitly requested. Do not edit or install before consent.
- Stop when: the target resolves to no files; the checkpoint is reached — important versus redundant findings summarized, nothing edited yet; a repair, a scope widening, or an install needs consent; savage/nuclear tone was not requested; or evidence is pattern-only — report the lead with its confidence instead of asserting exploitability, latency, or outage.

## Severity
- Capital offenses: confirmed secret exposure, injection/RCE paths, data loss or corruption, auth/access bypass.
- Felonies: risky security controls, N+1 or hot-path performance damage, brittle async/concurrency, dangerous coupling, change-blocking god functions.
- Crimes: broad type abuse, hidden state, poor errors, missing tests around risky logic.
- Slop: duplicate ceremony, AI-ish verbosity, unclear naming, style residue that slows maintenance.
- Misdemeanors: TODO fossils, console logs, formatting noise.
If there are 20+ issues, triage the top 10 by impact and confidence, then separate important findings from redundant noise.

## Smart routes — load only what the current step needs
- When you have the target and are ready to inspect it, load `references/roast-playbook.md` — the phase-by-phase run through inspection, inventory, autopsy, and the pre-fix checkpoint.
- When building the inventory and ranking generic smells, load `references/sin-catalog.md`; for language-specific patterns or structural queries load `references/language-sins.md` — choose evidence appropriate to the code.
- When the user picks repairs at the checkpoint, load `references/redemption-flow.md` — redeem findings through consent-gated fixes and verification.
- When scope spans a monorepo or many categories, load `references/parallel-roasting.md` — split the inspection and inventory across workers without duplicating findings.
- When research tooling is needed, load `references/octocode.md` and use `octocode-research` if available — verify before joking; mark reduced coverage otherwise.
- When improving this skill, prefer `octocode-graph-eval`; otherwise load `references/improve-loop.md` — require an accept/revert criterion.

## Related routes
- Use `octocode-research` for evidence gathering; `octocode-graph-eval` to measure roast usefulness; `octocode-prompt-optimizer` only for tone/instruction wording.
- Use `octocode-skills` when changing this skill folder.

## Output
Use: `Top roast`, `Important findings`, `Redundant / low-value findings`, `Autopsy`, `Redemption paths`, `Fix checkpoint`. Each finding includes `file:line`, evidence, impact, confidence, and repair move.

## Scripts
None — this skill is instruction-only. Evidence comes from `octocode-research` and the host's own repo tools; verification runs the target project's own checks.
