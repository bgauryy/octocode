# Octocode Roast

`octocode-roast` is for blunt, memorable code-quality critique. It lets an agent be sharp, funny, and hard to ignore while still staying cited, useful, and professionally safe.

The target is always the code pattern, never the person who wrote it.

## When to use

- The user explicitly asks to "roast my code", "find code sins", "give me a brutal review", or "tell me what is wrong with this".
- A normal review has not landed and the user wants the critique to be memorable.
- The goal is to expose the worst maintainability, safety, performance, or architecture smells.
- The user wants fix paths, but not automatic fixes yet.

Use `octocode-research` Review mode for sober PR review, normal diff review, or implementation. Use `octocode-rfc-generator` when the repair needs a formal plan.

## Features

- Evidence-backed findings with `file:line` citations.
- Severity tiers for major risks, design damage, brittle code, and cosmetic mess.
- Tone calibration from gentle to savage based on the user's request and code sensitivity.
- Secret-safe handling for credentials, security findings, and production-sensitive paths.
- Language-specific smell patterns and AST/search prompts.
- A top-offender autopsy when one pattern explains many issues.
- Redemption paths and a checkpoint before fixes are made.

## How it works

The skill follows this flow:

```text
TARGET -> INSPECT -> INVENTORY -> AUTOPSY -> CHECKPOINT -> REDEEM
```

It scopes the target, inspects code with Octocode or local tools, builds an issue inventory, ranks the most damaging patterns, and turns those into a sharp but actionable critique. Humor is added after evidence, not instead of evidence.

## Internal flow

1. Confirm the target and tone from the user's wording.
2. Search and read enough code to cite major findings.
3. Classify issues by severity and confidence.
4. Write the roast using code-focused language only.
5. Offer repair paths for the highest-impact issues.
6. Wait for the user's fix checkpoint before changing files.

## Installation

Install the published skill:

```bash
npx octocode skill --name octocode-roast
```

Install from a GitHub path or fork:

```bash
npx octocode skill --add bgauryy/octocode/skills/octocode-roast
```

## Benefits

- Makes technical debt memorable without turning feedback into personal insult.
- Keeps entertainment tied to real code evidence.
- Helps users see which issues matter most and how to start fixing them.
- Provides a bridge from critique to concrete remediation.

## For developers

Keep `SKILL.md` focused on trigger, safety rules, reference routing, and output shape. Put the inspection sequence in `references/roast-playbook.md`, reusable issue taxonomy in `references/sin-catalog.md`, language-specific searches in `references/language-sins.md`, tone calibration in `references/tone-personas.md`, and follow-up repair flow in `references/redemption-flow.md`.
