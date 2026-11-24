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
| Drafting steps | Put prerequisites first and give the result the reader sees after each consequential step. |
| Editing wording or repetition | Use the [style guide](references/style-guide.md) for disputed wording. Give each rule one home and link to it where readers need it. |
| Comparing choices or explaining a branch | Use the [format chooser](references/report-output.md#choose-the-format). |
| Before delivery | Run the [doc check](#check-before-delivery) and the project's own docs lint, if it has one. Verify changed examples and commands where safe and authorized, otherwise inspect their source. Identify which checks ran. |

Before removing text, check whether the reader can still choose the next action, find each rule and recognize success. Test a consequential line literally: what can fail if followed? Verify that risk against its source.

## Check before delivery

Run [doc-check.mjs](scripts/doc-check.mjs) from any folder on the changed Markdown files. It reads the files and writes nothing unless you pass `--write`:

```bash
node <skill-folder>/scripts/doc-check.mjs <changed.md>...
```

After renaming a heading or file, scan the folder that holds the inbound links and name each rename; `--write` rewrites each stale link whose replacement resolves:

```bash
node <skill-folder>/scripts/doc-check.mjs <docs-root> --renamed '<file.md>#<old heading>=<new heading>' --write
```

| Result | Action |
|---|---|
| Exit 0, `ok: true` | No mechanical finding; report the checks that ran. |
| `brokenLinks`, `missingAnchors` | Fix each entry; `suggestion` names the anchor a fragment was probably meant to be. |
| `missingCommands` | Fix the path or script name, unless the command runs the reader's own file (such as `app.mjs`) or a folder the text names. |
| `staleInboundAnchors` | An entry without `written: true` still needs its `link` replaced by `replacement`; `replacementOk: false` means the renamed heading or file does not exist yet. |
| `headingIssues`, `wordListHits`, `styleIssues` | Judge each in context with `wordGuidance` and `styleGuidance`; keep names and quoted examples as written. |
| Exit 2 | Fix the arguments the error names; `--help` lists the options. |

Deliver the document or findings with stable source anchors, checks run and unverified items. Use [output.md](output.md) for saved reports and [render-report.mjs](scripts/render-report.mjs) to render them. Use `octocode-research` for unresolved code claims, `octocode-rfc-generator` for open decisions and `octocode-skills` for skill instructions and packaging.
