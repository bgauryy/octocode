---
name: octocode-prompt-optimizer
description: "Use when prompts, SKILL.md, AGENTS.md, or agent instructions need optimization, gates, enforcement, or reliability fixes."
---

# Prompt Optimizer
Improve instructional prompts, docs, and agent instructions with prompt-engineering best practices — strengthen enforcement, add gates and output formats, shorten wording — while preserving the original intent.

## When to use
- Creating or improving a prompt, `SKILL.md`, or `AGENTS.md`.
- An agent skips steps, ignores instructions, or outputs inconsistently.
- Instructions lack enforcement, gates, or a defined output format.
- Text is verbose and could say the same thing in fewer tokens.

## Core flow
`READ → UNDERSTAND → RATE → FIX → VALIDATE → OUTPUT` — one gate per step; pass each gate before the next.

| Step | Gate output before proceeding |
|------|-------------------------------|
| READ | Whole input read; type + line count noted |
| UNDERSTAND | Goal, logical parts, and flow documented; unknowns asked |
| RATE | Issues table with severities + best-practices score produced |
| FIX | All Critical + High issues fixed in priority order |
| VALIDATE | Every required check passes; intent unchanged |
| OUTPUT | Chosen output variant + change summary emitted |

Mode: use **Full Path** (every gate separate) for multi-section, ambiguous, or high-risk prompts; **Fast Path** (READ+UNDERSTAND and RATE+FIX may combine) for short, low-risk ones. When unsure, use Full Path. VALIDATE and intent-preservation are never skipped.

## Tooling
When the prompt cites commands, flags, file paths, tool names, or schemas, verify them before rewriting.
If verification needs Octocode-backed code, package, GitHub, or tool research, use `octocode-research` when installed. If missing, use https://github.com/bgauryy/octocode/tree/main/skills/octocode-research or install with `npx octocode skill --name octocode-research`; otherwise use host tools and flag unverified claims.

## Non-negotiables
- Preserve working logic and intent — never alter what the prompt does without user approval.
- Follow the gates in order; never skip a gate, checkbox, or VALIDATE.
- Strengthen only critical rules; keep "should/prefer" for genuinely optional guidance.
- Density over length: cut only no-signal tokens; keep exact commands, versions, triggers, and structure.
- Do not bloat: target under 10% line increase; justify any overage in VALIDATE.
- Write only after VALIDATE passes; if review-only or no safe write tool exists, return the optimized text or a patch-style delta and state that no files changed.

## Reference map
- `references/gates.md` — when running the READ and UNDERSTAND gates.
- `references/rate.md` — when rating: issue categories plus the weak-word reference.
- `references/fix.md` — when fixing: command strength, Triple Lock, reasoning block, gate template, conciseness pass.
- `references/conciseness-toolkit.md` — when shortening a wordy or indirect line without losing logic.
- `references/attention.md` — when sharpening word choice, isolating a span with section tags (`<example>`), or ordering so critical rules land where attention peaks.
- `references/validate.md` — when validating all fixes before output.
- `references/output.md` — when formatting the final document and change report.
- `references/patterns.md` — when resolving conflicts or choosing high-value instruction patterns.
