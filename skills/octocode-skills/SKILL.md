---
name: octocode-skills
description: "Use when Agent Skills/SKILL.md need finding, comparison, review, creation, repair, install, sync, or trigger tuning."
---

# Octocode Skills

tools: `npx octocode` / `octocode-mcp`
related-skill: `octocode-research`
output: `<workspace>/.octocode/` for workspace work | `<home>/.octocode/` when no workspace applies
routes: load/run a reference, doc, script, or scheme only when it changes the next action; otherwise keep the rule here.

Operate on standalone Agent Skill folders: `SKILL.md` plus optional references, scripts, assets, and machine-readable schemes.
Flow: `UNDERSTAND → INSPECT → ACT → VERIFY`. Discover or compare candidates only when the source is unresolved; ask only for missing scope or authority. For review-only requests, report findings without applying edits.

Reviews: `<output>/octocode-skills/`; scratch: `<output>/tmp/octocode-skills/`. Chat-only recommendations stay in chat; approved edits, installs, symlinks, and configuration keep their gated destinations.

## Rules
- `SKILL.md` owns entry decisions and shared constraints; references own conditional detail. Use `scheme/<contract-name>.json` only when the skill needs a machine-readable contract: one valid top-level JSON object per file. Keep each shipped file reachable through a route that explains when to use it.
- Every lobby declares tools, one related-skill, the workspace-versus-home output decision, and the actionable route-value rule. Installing a missing related skill still requires authority.
- Inspect the real skill before quoting, judging, or installing it. Identify candidates by path and require authority for writes.
- Stop discovery when one fit is clear, further angles add no evidence, a winner needs user judgment, or approval is pending.
- Ship a standalone folder: local references stay inside it, and every shipped file is reachable from the lobby, README, or another used file. Remove duplicate, development-only metadata, probe, and scratch files. Core commands must work alone; optional sibling integrations must declare setup and pass isolated absent/present dependency checks.

## Smart routes — load only what the current step needs
- At UNDERSTAND, identify the requested skill operation, scope, source, and write authority before choosing a route.
- When discovering, load `references/search-playbook.md`; choose a source with `references/discovery-surfaces.md`, parse manifests with `references/discovery-manifests.md`, and recover with `references/recovery.md` — search broadly enough without inventing candidates.
- When judging, load `references/quality-rubric.md` for content fit, and `references/quality-signals.md` for adoption/recency; when recommending, present through `references/output-format.md` — rank evidence, not popularity alone.
- When designing structure or a `scheme/` contract, load `references/skill-anatomy.md`; write with `references/skill-authoring.md`, extract deterministic work with `references/skill-scripts.md`, and tune activation with `references/description-tuning.md` — keep the lobby lean and triggers strong.
- When improving, load `references/skill-improve.md`; choose review/refactor mode with `references/self-improvement.md` and clean with `references/skill-cleanup.md`. Use `octocode-eval-benchmark`; if unavailable, freeze goal/KPI/baseline and require comparable accept/revert evidence — preserve intent and measurable acceptance.
- Before done, load `references/skill-review.md`; interpret findings with `references/skill-review-rules.md` — check navigation, useful content, and standalone execution. Assess advisory formatting warnings in context.
- When reviewing lifecycle automation, load `references/hooks.md`; when adding it, load `references/hooks-add.md`, and use `assets/hooks/` — map the correct host event and avoid silent no-ops.
- When installing, load `references/install-gates.md`, then `references/install-destinations.md`; remote sources use `references/fetch-remote.md`, local creation uses `references/create-local-skill.md`, and vendor links use `references/skill-sync.md` — secure approval, destination, and provenance before writes.
- When evidence needs code/package/repository research, use `octocode-research`; if unavailable, use `octocode-mcp` or `npx octocode`, inspect live context/schema once, and follow executable continuations — do not duplicate tool contracts.
- When tracing source provenance, load `references/references.md`; when authoring a source appendix, start from `references/references-template.md` — keep claims auditable without bloating instructions.

## Related routes
- Use `octocode-research` to verify candidates; `octocode-prompt-optimizer` to improve wording; `octocode-eval-benchmark` to measure behavior.
- Use `octocode-rfc-generator` before a large skill-system redesign.

## Scripts and verification
- Run `scripts/skill-review.mjs` after any create/edit — zero ERROR is required. <!-- style-lint: ignore-line passive-voice -->
- Run `scripts/skill-sync.mjs` after inspecting its dry-run and confirming that existing authorization covers the source, destinations, and conflict policy. Ask only for missing authority.
- `scripts/skill-lint.mjs` is an alias for `scripts/skill-review.mjs` — same gate under the older name.
- A skill script needing Octocode home or env imports `./octocode-config.mjs`, a build artifact injected by `packages/octocode-config` into every skill that imports it relatively — never import `@octocodeai/config` from a skill, or the folder breaks once installed alone.
- When wiring a hook, copy `assets/hooks/example-hook.sh` into the target skill's hook-script directory and route that internal file from frontmatter.

Follow the approval and destination routes for creation or installation. Then review the result before reporting done.
