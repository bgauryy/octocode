# Write and improve skills

Load when creating a skill or changing its description, instructions or resource layout.

## Start from the task

1. State the recurring task, intended user/host, inputs, permissions and observable result. Search available skills first; extend the owning skill rather than create overlapping activation surfaces. A one-off answer or a repository-wide rule may not need a skill.
2. Write representative requests before drafting: explicit and implicit intent, missing input, a neighboring skill and a request needing no skill. For each, record expected selection, action, output and check. Preserve a baseline for revisions.
3. Write the smallest working entry. Add a reference only when a task branch needs detail; add a script for a deterministic operation worth reusing. Keep conditions beside actions and explain only what changes the decision.
4. Follow the checking route in `SKILL.md`. Keep or revise based on the failed task, not a preference for shorter prose.

## Discovery: name and description

The listing is the first routing surface; the body cannot rescue a description the host never selects. Inspect the listing actually served by the target host, including overrides or cached text, not only frontmatter.

- Keep a stable, distinctive `name` matching the folder: 1–64 lowercase ASCII letters, digits or single hyphens; no leading or trailing hyphen.
- Lead the `description` with a recognizable task or situation, then the distinctive capability. Include natural user terms and implicit intents, not an exhaustive keyword list. State both what it does and when to use it.
- Distinguish likely neighbors without claiming their work. Keep setup, commands and implementation steps in the body. Do not repeat the name as marketing.
- The format limit is 1,024 characters, not a target. Use the shortest wording that separates intended requests from near-misses; compare with neighboring descriptions.
- Public Octocode skills require one JSON-string `description:` line because the native build embeds it. This is a repository rule, not a general Agent Skills requirement. Check host-specific frontmatter against its loader.

Example: “Use when a recurring release workflow needs an Agent Skill, or an existing SKILL.md is hard to discover. Creates and checks the trigger, instructions and resource routes.” A generic “helps with workflows” cannot separate skill authoring from executing a release.

## Lobby and files

The lobby is the body of `SKILL.md`: it should reveal the next action without reading a directory tree.

| Surface | Owns |
|---|---|
| `SKILL.md` | Starting action, essential constraints, task branches, result and completion check |
| `references/<task-or-domain>.md` | Detail for a named branch, with its load condition and next action |
| `scripts/` | Reusable executable operations, dependencies, arguments, output, errors and safe retries |
| `assets/` | Templates or static input a named step consumes |
| `output.md` | Octocode multi-file result contract; a single-file skill keeps this inline |
| `README.md` | Human setup or usage only when it answers something the entry does not |

Use descriptive filenames and link labels: `check-skills.md` for validation, not `advanced.md`, `guide2.md` or a generic `utils.md`. A reader should predict the content without opening it. Group by task or domain, not arbitrary sequence numbers.

Link task-critical references directly from the lobby, ideally one read away. A specialized format guide can remain behind the optional output branch. Split by independent load conditions; merge files the same reader always needs together. Add a contents list when a long reference cannot be scanned readily. Do not add tiny pages solely to shrink the entry.

A multi-resource lobby uses `When | Read or run | Why`. Say whether to read a script or run it; routine execution should not require loading its implementation. Preserve exact commands, dependencies, permissions, output checks and failure recovery. Use relative Markdown links resolved from the containing file. Bundle required resources; name optional neighboring skills without assuming sibling folders are installed.

## Minimal single-file template

Replace the example task and checks with verified project behavior; do not leave generic placeholders in a shipped skill.

```markdown
---
name: release-checklist
description: "Use when preparing a release checklist from repository release instructions. Lists verified gates and unresolved blockers; does not publish."
---

# Release Checklist

Read the repository release instructions and identify the requested release scope.
If scope is ambiguous, resolve it before drafting commands. Do not publish or change credentials.

1. Map each release gate to its owning instruction and command.
2. Separate verified prerequisites from unresolved blockers.
3. Check that every command exists and matches the selected scope.

Return the ordered checklist with source links, checks run and unchecked blockers.
Done when each gate has a verified command or an explicit unresolved dependency.
```

For a multi-file Octocode skill, add only the resources its branches need and link `output.md`. Report skills use `## Saved report` there and link the bundled `references/report-output.md` contract. Configuration and shared templates come from their canonical owner; regenerate copies and test the shipped result.

## Standards and limits

The [Agent Skills specification](https://agentskills.io/specification) owns frontmatter and layout. Optional fields include `license`, `compatibility` (≤500 characters), string-map `metadata` and experimental space-separated `allowed-tools`; support varies by host. Use a YAML-aware validator for advanced YAML or publication.

[Official authoring guidance](https://platform.claude.com/docs/en/agents-and-tools/agent-skills/best-practices) supports concise instructions, descriptive filenames, shallow references and real-task evaluations. Its 500-line entry guidance is an upper-bound recommendation, not a size goal or proof of quality. Keep dates or versions only when they govern applicability. Never remove safety checks or return success-shaped defaults to make a workflow shorter.
