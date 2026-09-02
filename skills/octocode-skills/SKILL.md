---
name: octocode-skills
description: "Use when working on Agent Skills (SKILL.md folders): find, compare, rate, review, create, improve, repair, install, or sync skills from local workspaces, registries, or remote sources. For wording-only prompt polish without folder ops, use octocode-prompt-optimizer."
---

# Octocode Skills

Operate on standalone Agent Skill folders: `SKILL.md` plus optional references, scripts, and assets.
Flow: `UNDERSTAND → DISCOVER → INSPECT → JUDGE → RECOMMEND → USER GATE → ACT → CLEANUP → REVIEW → VERIFY`.

Workspace output contract: chat-only recommendations stay in chat. New reviews, comparison reports, or fetched scratch artifacts default to `<workspace>/.octocode/octocode-skills/` or `<workspace>/.octocode/tmp/octocode-skills/`. Approved skill source edits, installations, symlinks, and configuration keep their gated destinations. Never fall back to a user-level Octocode home for artifacts.

## Operating rules
- `SKILL.md` owns workflows and routes; each reference owns one concept. List every reference and runnable script in the lobby with its use condition.
- Inspect the real skill before quoting, judging, or installing it. Identify candidates by path and require authority for writes.
- Stop discovery when one fit is clear, further angles add no evidence, a winner needs user judgment, or approval is pending.
- Ship a standalone folder: local file references stay inside it, and every shipped file is reachable from the lobby, README, or another used file. Remove duplicate, development-only metadata, probe, and scratch files; name optional sibling skills without depending on their files.

## Smart routes — load only what the current step needs
- At UNDERSTAND, identify the requested skill operation, scope, source, and write authority before choosing a route.
- When discovering, load `references/search-playbook.md`; choose a source with `references/discovery-surfaces.md`, parse manifests with `references/discovery-manifests.md`, and recover with `references/recovery.md` — search broadly enough without inventing candidates.
- When judging, load `references/quality-rubric.md` for content fit and `references/quality-signals.md` for adoption/recency; when recommending, present through `references/output-format.md` — rank evidence, not popularity alone.
- When designing structure, load `references/skill-anatomy.md`; write with `references/skill-authoring.md`, extract deterministic work with `references/skill-scripts.md`, and tune activation with `references/description-tuning.md` — keep the lobby lean and triggers strong.
- When improving, load `references/skill-improve.md`; choose review/refactor mode with `references/self-improvement.md`, clean with `references/skill-cleanup.md`, and use `references/improve-loop.md` only if `octocode-eval-benchmark` is unavailable — preserve intent and require measurable acceptance.
- Before done, load `references/skill-review.md`; interpret findings with `references/skill-review-rules.md` — enforce structure, navigation (lobby shows every chunk, routes state when/why, chunks declare entry and next hop, flow phases routed), prose, and standalone-folder gates.
- When reviewing lifecycle automation, load `references/hooks.md`; when adding it, load `references/hooks-add.md` and use `assets/hooks/` — map the correct host event and avoid silent no-ops.
- When installing, load `references/install-gates.md`, then `references/install-destinations.md`; remote sources use `references/fetch-remote.md`, local creation uses `references/create-local-skill.md`, and vendor links use `references/skill-sync.md` — secure approval, destination, and provenance before writes.
- When evidence needs code/package/repository research, load `references/octocode.md` — delegate research mechanics instead of duplicating them.
- When tracing source provenance, load `references/references.md`; when authoring a source appendix, start from `references/references-template.md` — keep claims auditable without bloating instructions.

## Related routes
- Use `octocode-research` to verify candidates; `octocode-prompt-optimizer` to improve wording; `octocode-eval-benchmark` to measure behavior.
- Use `octocode-rfc-generator` before a large skill-system redesign.

## Scripts and verification
- Run `scripts/skill-review.mjs` after any create/edit — zero ERROR is required.
- Run `scripts/skill-sync.mjs` only after a dry-run and human approval — synchronize one source safely.
- `scripts/skill-lint.mjs` is an alias for `scripts/skill-review.mjs` — same gate under the older name.
- A skill script needing Octocode home or env imports `./octocode-config.mjs`, a build artifact injected by `packages/octocode-config` into every skill that imports it relatively — never import `@octocodeai/config` from a skill, or the folder breaks once installed alone.
- When wiring a hook, copy `assets/hooks/example-hook.sh` into the target skill's hook-script directory and route that internal file from frontmatter.

When creating or installing, follow the approval and destination routes, then review the result before reporting done.
