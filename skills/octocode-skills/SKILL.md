---
name: octocode-skills
description: "Use when any Agent Skill lifecycle work is needed: discover, compare, inspect, review, create, improve, repair, install, or synchronize a SKILL.md folder from local workspaces, registries, or remote sources."
---

# Octocode Skills

Operate on standalone Agent Skill folders: `SKILL.md` plus optional references, scripts, assets, and evals.
Flow: `UNDERSTAND → DISCOVER → INSPECT → JUDGE → RECOMMEND → USER GATE → ACT → CLEANUP → REVIEW → VERIFY`.

## Lobby rules
- `SKILL.md` owns workflows, hard gates, and routes; each reference owns one concept and never restates the flow.
- MUST read and navigate between routed reference files to understand the complete flow; this skill is intentionally simplified into reference units for efficiency, including reference-to-reference handoffs.
- Inspect the real `SKILL.md` before quoting, judging, or installing; identify candidates by path and gate every write.
- Stop when one fit is clear, two High candidates need a winner, three angles add nothing, or user/auth approval is pending.
- Ship a standalone folder with one owner per concept; prune dead or duplicated material before done.

## Smart routes — load only what the current step needs
- When discovering, load `references/search-playbook.md`; choose a source with `references/discovery-surfaces.md`, parse manifests with `references/discovery-manifests.md`, and recover with `references/recovery.md` — search broadly enough without inventing candidates.
- When judging, load `references/quality-rubric.md` for content fit and `references/quality-signals.md` for adoption/recency; present through `references/output-format.md` — rank evidence, not popularity alone.
- When designing structure, load `references/skill-anatomy.md`; write with `references/skill-authoring.md`, extract deterministic work with `references/skill-scripts.md`, and tune activation with `references/description-tuning.md` — keep the lobby lean and triggers strong.
- When improving, load `references/skill-improve.md`; choose review/refactor mode with `references/self-improvement.md`, clean with `references/skill-cleanup.md`, and use `references/improve-loop.md` only if `octocode-eval` is unavailable — preserve intent and require measurable acceptance.
- Before done, load `references/skill-review.md`; interpret findings with `references/skill-review-rules.md` — enforce structure, routing, prose, and standalone-folder gates.
- When reviewing lifecycle automation, load `references/hooks.md`; when adding it, load `references/hooks-add.md` and use `assets/hooks/` — map the correct host event and avoid silent no-ops.
- When installing, load `references/install-gates.md`, then `references/install-destinations.md`; remote sources use `references/fetch-remote.md`, local creation uses `references/create-local-skill.md`, and vendor links use `references/skill-sync.md` — secure approval, destination, and provenance before writes.
- When evidence needs code/package/repository research, load `references/octocode.md` — delegate research mechanics instead of duplicating them.
- When tracing source provenance, load `references/references.md`; when authoring a source appendix, start from `references/references-template.md` — keep claims auditable without bloating instructions.

## Related routes
- Use `octocode-research` to verify candidates; `octocode-prompt-optimizer` to improve wording; `octocode-eval` to measure behavior.
- Use `octocode-awareness` for shared-repo edits; `octocode-rfc-generator` before a large skill-system redesign.

## Scripts and done gate
- Run `scripts/skill-review.mjs` after any create/edit — zero ERROR is required.
- Run `scripts/skill-sync.mjs` only after a dry-run and human approval — synchronize one source safely.

## Installation
When creating or installing, follow the routed approval and destination gates, then run the review before reporting done.
