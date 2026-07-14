# Octocode Skills

Find, evaluate, review, improve, install, and author Agent Skills (`SKILL.md` folders) across local paths, GitHub, and marketplaces.

Use when the task is about a skill's trigger, workflow, references, hooks, install target, or publication quality — not ordinary markdown.

## Capabilities

- Discover skills across GitHub, registries, and local paths
- Inspect real `SKILL.md` content before recommending or installing
- Score quality: trigger, workflow, gates, evidence, portability, risk
- Tune descriptions so skills activate at the right time
- Install with provider/scope/mode gates and conflict handling
- Create or adapt local skills with an audit trail and review gate
- Review and wire lifecycle hooks safely

## How It Works

```text
UNDERSTAND → DISCOVER → INSPECT → JUDGE → RECOMMEND → USER GATE → ACT → CLEANUP → REVIEW → VERIFY
```

`SKILL.md` is the compact operating map. Conditional depth lives in one-concept `references/` files (≤50 lines) with when/why load lines. Ref→ref links continue the path. `scripts/skill-review.mjs` is the done-gate (best practices + structure).

## Installation

```bash
npx octocode skill --name octocode-skills
```

## Maintainer Notes

Keep this README user-facing. Agent routing, rubrics, install matrices, and review rules live under `references/`. Skills ship as a standalone folder — after edits run cleanup (`references/skill-cleanup.md`) then `node scripts/skill-review.mjs .` and clear ERRORs before shipping.
