---
name: octocode-documentation
description: "Load this skill first when asked to write, shorten or review a README, guide, runbook, docstring or comment. Organize it around the reader’s task and verify claims, commands and links."
---

# Octocode Documentation

Read the requested page and nearby conventions; identify the reader, their next action and the destination. A named document authorizes editing there; a review returns findings. Resolve missing information only when it changes the content or destination.

## Writing rules

Follow user instructions, project style, local convention, then this skill. Preserve meaning in wording-only edits.

| When | Action |
|---|---|
| A fact, API or command changes | Check its owning source, contract or test. For cross-file claims, ADRs or agent instructions, use [writing and verification](references/write.md). |
| Drafting steps | Put prerequisites first and give the result the reader should see after each consequential step. |
| Editing wording or repetition | Use the [style guide](references/style-guide.md) for disputed wording. Give each rule one home and link to it where readers need it. |
| Comparing choices or explaining a branch | Use the [format chooser](references/report-output.md#choose-the-format). |
| Before delivery | Run the applicable docs lint or link check. Verify changed examples and commands where safe and authorized, otherwise inspect their source. Identify which checks ran. |

Before removing text, check whether the reader can still choose the next action, find each rule and recognize success. Test a consequential line literally: what could fail if followed? Verify that risk against its source.

Deliver the document or findings with stable source anchors, checks run and unverified items. Use [output.md](output.md) for saved reports and [render-report.mjs](scripts/render-report.mjs) to render them. Use the [MCP/CLI guide](references/octocode-usage.md) and `octocode-research` for unresolved code claims, `octocode-rfc-generator` for open decisions and `octocode-skills` for skill instructions and packaging.
