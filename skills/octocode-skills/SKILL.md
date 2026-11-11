---
name: octocode-skills
description: "Use when finding, creating, improving, reviewing, validating, installing or syncing Agent Skills and SKILL.md files. Covers concise entries, descriptions and triggers, overlapping activation, reference routing and standalone packaging. Use octocode-agentic for broader prompt behavior and controlled activation experiments."
---

# Octocode Skills

Identify the task, intended host and successful result. A review returns findings; creation or improvement edits the canonical source; finding a skill does not authorize installation.

## Resources

Read only the row needed now. Paths are relative to this skill folder; invoke scripts by that path, not by assuming the current directory is the skill.

| When | Read or run | Why |
|---|---|---|
| Find an existing skill or decide whether to reuse one | [Find skills](references/find-skills.md) | Search, inspect and recommend without installing |
| Create a skill or repair its description, entry or references | [Write skills](references/write-skills.md) | Authoring workflow, minimal template and file naming |
| Review, shorten or validate a skill or collection | [Check skills](references/check-skills.md) | Discovery-to-files audit, commands and behavior tests |
| Check structure only | Run [skill-review.mjs](scripts/skill-review.mjs) with `<target> --json` | Inspect errors and warnings; not an activation test |
| Install, adapt a fetched skill or sync local providers | [Install and sync](references/install.md) | Source inspection, destinations, conflicts and recovery |
| A dependency needs settings or credentials | [Configuration](references/configuration.md) | Resolve only the required configuration |
| Deliver findings, changes or a saved report | [Result formats](output.md) | Readiness, evidence, unchecked scope and optional rendering |

## Work loop

| Stage | Action | Check |
|---|---|---|
| Before writing | Record realistic requests, neighboring intent and a no-skill case; find existing coverage. | Expected action is clear; no duplicate owner. |
| Trace | Discovery → entry → selected reference/script → result. | Each decision has one owner; safety and verification remain. |
| Edit | Change the smallest canonical surface; regenerate shared resources. | Optional detail stays off the normal path. |
| Verify | Run structural checks, changed commands and standalone-bundle checks; compare the same tasks. | Smaller files or passing structure do not prove activation gains. |

## Done

Return **Ready** or **Needs work**, with findings or changes, checks run and unchecked scope. Distinguish structural evidence, editorial judgment and observed host behavior. Use numerical ratings only with defined criteria.

For prompt behavior beyond skill packaging, or measured activation and cost, use `octocode-agentic` when available. It is not required for a structural check.
