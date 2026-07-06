# Prompt Optimizer

A skill that helps agents optimize instructional prompts, `SKILL.md` files, and `AGENTS.md` documents. It strengthens weak enforcement, adds gates and output formats, and shortens wording — all while preserving the original intent. Use it whenever a prompt is unreliable, verbose, or an agent keeps skipping steps.

## Features
- **6-step gated flow** — READ → UNDERSTAND → RATE → FIX → VALIDATE → OUTPUT, with a checkpoint at each step.
- **Fast/Full modes** — combine gates for short, low-risk prompts; keep them separate for complex or high-risk ones.
- **Issue rating** — a categorized scan (weak words, missing enforcement, ambiguity, wordiness) with severities.
- **Command strengthening** — a strength hierarchy plus the Triple Lock pattern for critical rules.
- **Conciseness toolkit** — research-backed moves to shorten a line without losing logic, with guardrails against over-compression.
- **Attention & structure toolkit** — word choice, section tags (`<example>`), and ordering so critical rules land where a U-shaped attention curve peaks.
- **Intent preservation** — validation checks that block any change to what the prompt does.

## How it works
The skill routes from a lean `SKILL.md` map to focused references, loaded on demand:
- `references/gates.md` — READ and UNDERSTAND gates.
- `references/rate.md` — issue categories and the weak-word reference.
- `references/fix.md` — command strength, Triple Lock, reasoning block, gate template, conciseness pass.
- `references/conciseness-toolkit.md` — compression moves and over-compression guardrails.
- `references/attention.md` — word choice, section tags, and attention-aware ordering.
- `references/validate.md` — the validation checklist and Definition of Done.
- `references/output.md` — output variants and the change report.
- `references/patterns.md` — precedence, conflict resolution, and high-value patterns.

Agents load only the reference they need at each step, keeping the activation context small.

## For users and developers
Users invoke it with asks like "optimize this prompt" or "make this SKILL.md more reliable" and receive either a full rewrite or a patch-style delta plus a change summary. Developers and maintainers extend it by editing the references (each is single-purpose and under 150 lines); there are no scripts to maintain. Keep the repo source and the installed copy in sync after edits.

## Installation
Install with the Octocode skills CLI:

```bash
npx octocode skill --name octocode-prompt-optimizer
```

Or add it from a path or GitHub source:

```bash
npx octocode skill --add <github-path-or-local-path>
```
