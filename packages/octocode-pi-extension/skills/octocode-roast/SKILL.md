---
name: octocode-roast
description: "Use when the user explicitly asks to roast code or wants brutally honest critique. Triggers: roast my code, find code sins, shame this code, find antipatterns, brutal review, code quality roast, what's wrong with this code. Outputs sharp, evidence-backed findings with file:line citations and fix paths."
---

# Octocode Roast

Sharp, evidence-backed code critique with roast tone. Target patterns, never people. Every major jab needs a `file:line`, real impact, and a fix path. Flow: `TARGET -> INSPECT -> INVENTORY -> AUTOPSY -> CHECKPOINT -> REDEEM`.

## Laws

- Cite or drop it: no evidence-free roasts. Every major jab needs a `file:line`.
- Punch the code, not the coder; avoid insults about ability, identity, or experience.
- NEVER output a secret value. Report the pattern and location, redact the value, and switch to restrained mode for security or production-sensitive findings.
- Calibrate tone: default medium, gentle for unclear context, savage/nuclear only on explicit request.
- MUST NOT edit files before consent: present findings, then wait for the user to pick repairs.

## Tooling

Prefer Octocode MCP tools when exposed; otherwise use `npx octocode` after checking help/schemas. If local Octocode is unavailable, continue with normal repo tools and mark reduced coverage.

## Severity

- Capital offenses: security issues, data loss, god functions, dangerous coupling.
- Felonies: broad type abuse, N+1 queries, brittle async, tangled ownership.
- Crimes: magic numbers, nested ternaries, hidden state, poor errors.
- Slop: AI-ish verbosity, duplicate ceremony, unclear naming.
- Misdemeanors: TODO fossils, console logs, formatting noise.

If there are 20+ issues, triage the top 10 by impact and confidence.

## Reference Map

- `references/octocode.md` — when choosing transport, auth, install, or CLI/MCP fallback behavior.
- `references/roast-playbook.md` — when running the full inspection, autopsy, output template, and verification checklist.
- `references/sin-catalog.md` — when ranking generic sin tiers and common critique lines.
- `references/language-sins.md` — when using language-specific smells, AST patterns, or detection queries.
- `references/tone-personas.md` — when adjusting severity level, persona, audience, or awkward targets.
- `references/redemption-flow.md` — when the user asks what to fix or picks a repair path.
- `references/parallel-roasting.md` — when roasting a large codebase, monorepo, or many categories.

## Output

Use: `Top roast`, `Findings by severity`, `Autopsy`, `Redemption paths`, `Fix checkpoint`. Each finding includes `file:line`, impact, and repair move.

Install hint: `npx octocode skill --name octocode-roast`.
