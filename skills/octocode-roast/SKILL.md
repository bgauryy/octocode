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

```mermaid
flowchart LR
  T[TARGET] --> I[INSPECT] --> N[INVENTORY] --> A[AUTOPSY] --> C{CHECKPOINT}
  C -->|fixes authorized or chosen| R[REDEEM]
  C -->|critique only| S[Stop]
  T -. "target clear: phases, finding shape, output order" .-> P["roast-playbook.md"]
  I -. "monorepo or many independent categories" .-> PR["parallel-roasting.md"]
  N -. "severity labels, language leads" .-> SC["sin-catalog.md"]
  R -. "repairs chosen" .-> RF["redemption-flow.md"]
```
Skill map: dotted edges load a reference. No edit before consent: the request authorizes fixes or the user picks repairs at CHECKPOINT.

Reports: `<output>/octocode-roast/`; scratch: `<output>/tmp/octocode-roast/`. Chat-only critiques stay in chat; approved source edits keep their named paths.

## Rules
- Punch the code, not the coder: no insults about ability, identity, or experience.
- Cite or drop it: every major finding needs an exact anchor, mechanism, impact, confidence, and repair move. Pattern-only matches stay leads with stated confidence.
- Use explicit user targets first; widen to diff/repo scope only when no target exists or the user approves.
- Never reveal a secret; redact values and keep security or production-sensitive findings restrained.
- Rank by demonstrated impact and confidence: security, data loss, correctness, and user-visible performance outrank maintainability and taste.
- Match the requested tone; savage/nuclear language only on explicit request. Do not edit or install before consent.
- Stop when the target resolves to no files, a repair or scope expansion needs consent, or evidence cannot support the claimed impact.

## Routes
Pages (load when its map edge applies): `references/roast-playbook.md` · `references/parallel-roasting.md` · `references/sin-catalog.md` · `references/redemption-flow.md`
- Evidence comes from `octocode-research`; if unavailable, use `octocode-mcp` / `npx octocode` and mark reduced coverage. `octocode-eval-benchmark` measures usefulness, `octocode-prompt-optimizer` handles wording, `octocode-skills` owns changes to this folder. No scripts: verification runs the target project's own checks.
