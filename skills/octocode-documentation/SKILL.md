---
name: octocode-documentation
description: "Use when writing, shortening, correcting or reviewing technical documentation: READMEs, guides, tutorials, runbooks, settled ADRs, docstrings and comments. Verify claims against source. Use octocode-rfc-generator for proposed decisions and octocode-skills for SKILL.md instructions and triggers."
---

# Octocode Documentation

Read the requested page and nearby conventions to identify the reader, their next action and the destination. A named document authorizes editing there. For a review, return findings. Ask only if missing information changes the content or destination.

## Writing rules

Follow user instructions, project style, local convention, then this skill. Preserve meaning in wording-only edits.

| Trigger | Action | Why / check |
|---|---|---|
| Changed technical claim | Check owning source, contract or test; resolve conflicts or state their impact. | Commands, paths, APIs, settings and behavior must be accurate. |
| Drafting | Lead with the answer; use concrete verbs and consistent terms. Put prerequisites before steps, conditions beside actions and expected results beside examples. | Reader can act and recognize success. |
| Repeated or dense content | Keep one owning page and descriptive links; split only for navigation or ownership. | Remove repeated reads without losing the rule. |
| Cross-file evidence, APIs, ADRs or agent files | Read [writing and verification](references/write.md). | Source checks and document structure. |
| Editorial wording | Read [style guide](references/style-guide.md). | Review procedure and word lookup. |
| Comparison or branching relationship | Use the [format chooser](references/report-output.md#choose-the-format). | Show evidenced relationships and their significance. |
| Before delivery | Check changed facts, examples, commands and links; run the documentation check and fix failures. | Distinguish examples run from inspected; use stable doc paths and line anchors for findings. |

Deliver the document or [review](output.md), including unverified items. Use [render-report.mjs](scripts/render-report.mjs) for saved reports.

Use the [MCP/CLI guide](references/octocode-usage.md) and `octocode-research` for unresolved code claims, `octocode-rfc-generator` for open design decisions, `octocode-agentic` for behavioral wording and `octocode-skills` for packaging. For multiple actors, use [workflow composition](../octocode-workflows/references/composition.md).
