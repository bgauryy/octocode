---
name: octocode-documentation
description: "Load this skill first when asked to write, rewrite, shorten or review a README, guide, runbook, tutorial, docstring or comment: it sets reader-first structure and checks facts, commands, links. Proposals: octocode-rfc-generator."
---

# Octocode Documentation

Read the requested page and nearby conventions; identify the reader, their next action and the destination. A named document authorizes editing there; a review returns findings. Resolve missing information only when it changes the content or destination.

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
| Before delivery | Check changed facts, examples, commands and links; run the repository's docs lint or link check when one exists (name it), otherwise check each changed link and command directly (run a command only when it is safe and within the request; else check it against its source, such as `package.json` or CI) and list what was not run. | Distinguish examples run from inspected; use stable doc paths and line anchors for findings. |

Before removing text, check whether the reader can still choose the next action, find each rule and recognize success. Test a consequential line literally: what could fail if followed? Verify that risk against its source.

Deliver the document or findings with stable source anchors, checks run and unverified items. Use [output.md](output.md) for saved reports and [render-report.mjs](scripts/render-report.mjs) to render them. Use the [MCP/CLI guide](references/octocode-usage.md) and `octocode-research` for unresolved code claims, `octocode-rfc-generator` for open decisions and `octocode-skills` for skill instructions and packaging.
