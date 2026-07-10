---
name: octocode-skills
description: "Use when finding, researching, reviewing, improving, installing, or creating Agent Skills and SKILL.md folders across local paths, GitHub, or marketplaces."
---

# Octocode Skills

Operate on Agent Skills (`SKILL.md` plus optional references, scripts, assets).
**Lobby rule:** all workflows live here; refs are one-concept depth — no flow restatement, no overlapping owners, load one ref at a time.
Flow: `UNDERSTAND → DISCOVER → INSPECT → JUDGE → RECOMMEND → USER GATE → ACT → CLEANUP → REVIEW → VERIFY`.
Hard rules: inspect real `SKILL.md` before recommend/install/quote; identify by path; gate every write; one owner per concept.
Stop when one fit, two High pick a winner, three angles add nothing, or a gate is pending. Skills ship as a **standalone folder** — prune dead files before done (`references/skill-cleanup.md`).

## Workflows → load

- when discovering: `references/search-playbook.md` — fan-out and angles before shopping
- when shopping registries: `references/discovery-surfaces.md` — pick the right surface
- when parsing manifests/CLIs: `references/discovery-manifests.md` — formats and installers
- when judging fit: `references/quality-rubric.md` — dimensions plus High/Med/Low
- when ranking evidence: `references/quality-signals.md` — installs/recency beat stars
- when presenting: `references/output-format.md` — cards and next-step gate
- when authoring structure: `references/skill-anatomy.md` — progressive disclosure map
- when writing instructions: `references/skill-authoring.md` — sources, control, patterns
- when bundling scripts: `references/skill-scripts.md` — deterministic code over prose
- when tuning description: `references/description-tuning.md` — trigger is discovery signal
- when improving any skill: `references/skill-improve.md` — lobby owns flow; dedupe; ≤50 one-concept
- when cleaning before ship: `references/skill-cleanup.md` — prune orphans/dupes; standalone folder only
- when rating/refactor: `references/self-improvement.md` — mode gate and READ→REPORT
- before calling done: `references/skill-review.md` — full review gate (best practices + rules)
- when interpreting findings: `references/skill-review-rules.md` — ERROR/WARN codes to fixes
- when reviewing hooks: `references/hooks.md` — host surfaces and event contract
- when adding a hook: `references/hooks-add.md` — wire frontmatter plus templates
- when installing: `references/install-gates.md` — user gates and checklist
- when choosing destinations: `references/install-destinations.md` — maps provider and scope paths
- when syncing to vendors: `references/skill-sync.md` — dry-run then human `--approve`
- when fetching remote: `references/fetch-remote.md` — clone, scan, then write
- when creating local: `references/create-local-skill.md` — plan, approve, write, review
- when search fails: `references/recovery.md` — broaden once, then report gap
- when needing local or external research evidence: `references/octocode.md` — route through octocode-research or Octocode

## Next skills
- `octocode-prompt-optimizer` — tighten SKILL.md / description / contracts after create or improve
- `octocode-research` — evidence when judging fit or prior-art skills
- `octocode-awareness` — skill-evolution gates; shared-repo install continuity

## Scripts

- `scripts/skill-review.mjs` — before reporting any create/edit done; best-practices + structure review gate
- `scripts/skill-sync.mjs` — when symlinking a local skill to vendors; dry-run first, `--approve` only after human OK
- `assets/hooks/` templates — when adding a lifecycle hook; copy after `references/hooks-add.md`

## Installation

When installing or creating: follow the install/create routes above, then review before done.
