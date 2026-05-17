# Agent Skills Guide

Use this reference when evaluating, improving, or creating Agent Skills. Keep `SKILL.md` focused on activation-critical instructions and load this file only when the task is about skill quality, structure, descriptions, scripts, or progressive disclosure.

## What Agent Skills Are

Agent Skills are a lightweight, open folder format for extending AI agents with specialized knowledge and workflows.

At minimum, a skill is a folder containing `SKILL.md`. That file includes frontmatter metadata (`name` and `description`) plus instructions for a specific class of tasks. Skills may also bundle scripts, reference materials, templates, and other resources.

```text
my-skill/
|-- SKILL.md          # Required: metadata + instructions
|-- scripts/          # Optional: executable code
|-- references/       # Optional: documentation
|-- assets/           # Optional: templates, resources
`-- ...               # Optional additional files or directories
```

## Why Skills Help Agents

Skills package procedural knowledge and team-, company-, user-, or domain-specific context into portable, version-controlled folders that agents load on demand.

High-value skills provide:

- Domain expertise: specialized knowledge the model would not reliably know by default.
- Repeatable workflows: multi-step procedures with consistent, auditable execution.
- Cross-product reuse: one skill can work across skills-compatible agents.

## Progressive Disclosure

Agents load skills in three stages:

1. Discovery: at startup, the agent sees only each skill's `name` and `description`.
2. Activation: when the user task matches the description, the agent reads full `SKILL.md`.
3. Execution: the agent follows `SKILL.md`, loading bundled resources or running scripts only when needed.

Design implication: every token in `SKILL.md` competes with conversation and system context. Put always-needed instructions in `SKILL.md`; move conditional details to `references/`, `scripts/`, or `assets/` and state when to load them.

## Source Material For Good Skills

Start from real expertise, not generic best practices.

Strong sources:

- Real completed tasks and the sequence that worked.
- User corrections and preferences from hands-on runs.
- Input/output examples and expected formats.
- Internal docs, runbooks, style guides, schemas, API specs, and configuration files.
- Code review comments, issue trackers, incident reports, and fixes from version control history.
- Real-world failure cases and their resolutions.

Avoid skills that only say vague things like "handle errors appropriately" or "follow auth best practices." Replace generic advice with concrete tools, commands, edge cases, and recovery procedures.

## Context Discipline

Add what the agent lacks; omit what the agent already knows.

Ask for each instruction: "Would the agent likely get this wrong without the skill?" If not, cut it. If unsure, test it. If the agent already performs the task well without the skill, the skill may not add value.

Good skills are coherent units of work. Avoid scopes that are too narrow and force many skills to load, or too broad and trigger imprecisely.

Aim for moderate detail: concise, stepwise guidance with a working example usually beats exhaustive documentation. Keep `SKILL.md` under 500 lines and 5,000 tokens; for this repository, prefer under 300 lines when practical.

## Progressive Reference Files

Move long or conditional material into `references/` when it is not needed on every activation.

Every reference link in `SKILL.md` must say when to load it, for example:

```text
Read `references/api-errors.md` if the API returns a non-200 response.
```

A generic "see references/" is too weak because the agent may not know which file matters.

Keep non-obvious gotchas in `SKILL.md` if the agent must know them before it can recognize the trigger. Otherwise put them in a reference file with an explicit load condition.

## Calibrating Control

Match specificity to task fragility.

Use flexible guidance when several approaches are valid and the task tolerates variation. Explain why the rule exists so the agent can adapt.

Use prescriptive instructions when operations are fragile, safety-sensitive, destructive, or order-dependent. For fragile operations, provide exact commands and say not to modify them.

Provide defaults, not menus. Pick the default tool or approach, then mention alternatives only as escape hatches.

Favor reusable procedures over one-off answers. A skill should teach an approach for a class of tasks, while still including concrete formats, constraints, and tool rules where they matter.

## Effective Instruction Patterns

Use gotchas for environment-specific facts that correct likely agent mistakes:

- Naming differences that refer to the same object.
- Health checks that give misleading success.
- Soft-delete filters or required query conditions.
- Tool behavior that violates common assumptions.

Use templates when output shape matters. Short templates can live in `SKILL.md`; long or conditional templates belong in `assets/` with explicit load instructions.

Use checklists for dependent multi-step workflows. Include validation gates so the agent can track progress and avoid skipping steps.

Use validation loops:

1. Do the work.
2. Run a validator, script, reference checklist, or self-check.
3. Fix issues if validation fails.
4. Repeat until validation passes.
5. Proceed only after validation passes.

Use plan-validate-execute for batch, stateful, or destructive operations. The plan must be checked against a source of truth before execution.

## Bundling Scripts

Use one-off commands when an existing tool already does the job and the command is simple. Pin versions when reproducibility matters, and state prerequisites.

Move complex or repeatedly reinvented logic into `scripts/`. A good script for agents:

- Accepts input via flags, environment variables, files, or stdin.
- Never requires interactive prompts.
- Provides concise `--help` output with examples.
- Emits helpful errors that say what was wrong, what was expected, and what to try.
- Sends structured data to stdout and diagnostics to stderr.
- Is idempotent or safe to retry.
- Rejects ambiguous input instead of guessing.
- Supports `--dry-run` for destructive or stateful operations.
- Uses meaningful exit codes when useful.
- Keeps output bounded or supports pagination/output-file flags.

Reference scripts from `SKILL.md` with paths relative to the skill root, for example `scripts/validate.sh`.

## Description Optimization

The `description` field is the primary trigger. At startup, agents see only `name` and `description`, so the description must tell the agent when to load the skill.

Good descriptions:

- Use imperative phrasing: "Use this skill when..."
- Focus on user intent, not implementation internals.
- Include non-obvious trigger situations where the user may not name the domain directly.
- Stay concise and under the 1024-character limit.
- Avoid being so broad that near-miss prompts trigger the skill.

Test descriptions with realistic eval queries:

- Should-trigger prompts: vary phrasing, typos, explicitness, detail, and task complexity.
- Should-not-trigger prompts: include near-misses that share keywords but need a different skill.
- Use train/validation splits so edits do not overfit to the test prompts.
- Run multiple times when behavior is nondeterministic and compare trigger rates.

Optimization loop:

1. Evaluate current description on train and validation sets.
2. Identify train failures: missed triggers and false triggers.
3. Revise for the general category of failure, not exact query keywords.
4. Keep the description under 1024 characters.
5. Select the best iteration by validation pass rate.
6. Sanity-check with fresh queries that were not used during optimization.

## Quality Signals Beyond Stars

When ranking candidates, prefer evidence-based signals over raw star count:

- **Install count** — `https://www.skills.sh` leaderboard ranks by installs. A skill with high installs but modest stars is often more battle-tested than the reverse.
- **Per-skill index page** — check `https://www.skills.sh/<owner>/<repo>/<skill-name>` (or `https://www.skills.sh/<org>/skills/<skill-name>` when the repo is named `skills`) for install count, install command, audit badge, and related skills.
- **Recency** — `pushed:>YYYY-MM-DD` on GitHub. Skip skills with no commits in the last 12 months unless the user wants archival.
- **Audit badges** — skills.sh exposes Gen Agent Trust Hub pass/fail; Microsoft uses the Sensei rubric (triggers + anti-triggers + compatibility scored Low/Medium/High).
- **Registry-side fields** — `aiskillstore.io` exposes `match_reasons`, `downloads_7d`, `days_since_update`, and a `/similar` endpoint for overlap-ranked alternatives.
- **Demand signal** — `aiskillstore.io/v1/demand/most-wanted` shows what users searched for and did not find; useful when deciding whether to adapt vs create.

For full registry details, manifest formats, and CLI installers, load `discovery-surfaces.md`.

## External Documentation Index

The full documentation index is available at `https://agentskills.io/llms.txt`. Use that index only when external web access is allowed and current upstream details matter.
