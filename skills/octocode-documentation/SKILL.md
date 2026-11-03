---
name: octocode-documentation
description: "Write, revise or review READMEs, guides, tutorials, runbooks, ADRs for settled decisions, docstrings and code comments. Verify technical claims against their owning source and keep documentation useful to its reader."
---

# Octocode Documentation

Read the requested page and nearby conventions to identify the reader, their next action and the destination. A named document authorizes editing there. For a review, return findings. Ask only if missing information changes the content or destination.

## Writing rules

Follow the user, then project style, local convention and this skill. Preserve meaning in wording-only edits. Verify changed commands, paths, APIs, settings and behavior against their owning code, contracts or tests. Resolve conflicting sources with evidence or state the discrepancy and its impact.

Lead with the answer. Use concrete verbs and consistent terms. Put prerequisites before ordered steps, conditions beside actions and expected results beside examples. Give each fact one owning page and a descriptive link. Split pages only to improve navigation or ownership. Use stable paths for durable docs and line anchors for review findings.

Use the [writing and verification guide](references/write.md) for cross-file evidence, APIs, settled ADRs and agent instruction files. The [style guide](references/style-guide.md) covers editorial review and word lookup. For comparisons or diagrams, use the [format chooser](references/report-output.md#choose-the-format). Draw evidenced relationships and explain their significance.

Check changed facts, examples, commands and links. Run the project's documentation check, correct failures and distinguish examples run from examples inspected. Deliver the document in its own format or a [review with evidence and unverified items](output.md). Use the [renderer](scripts/render-report.mjs) for saved reports.

Use the [MCP/CLI guide](references/octocode-usage.md) and `octocode-research` for unresolved code claims, `octocode-rfc-generator` for open design decisions, `octocode-agentic-prompts` for behavioral wording and `octocode-skills` for packaging. For multiple actors, use [workflow composition](../octocode-workflows/references/composition.md).
