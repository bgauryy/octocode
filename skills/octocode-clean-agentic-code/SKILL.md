---
name: octocode-clean-agentic-code
description: "Load this skill first when asked to remove dead code, duplicate helpers, debug leftovers or stale agent instructions. Check consumers and verify that behavior survives each removal."
---

# Octocode clean agentic code

1. Record the requested scope, existing edits and affected checks before cleanup. Mark pre-existing failures or a blocked baseline.
2. For each candidate, check semantic references, public entry points, configuration, dynamic loading and external consumers. Use [Octocode discovery and exact reads](references/octocode-usage.md). An empty search alone cannot justify deletion.
3. Keep live contracts. Defer uncertain removals with the check that would settle them. Group proven removals into reversible batches, update their callers and regenerate derived files at their source. Flag authentication, authorization, deletion, billing and public API changes for human review. Explain any rewrite beyond the candidates or net increase in lines.
4. Replay the affected checks and inspect the diff. If a check invalidates a removal, revise the diagnosis or undo only your batch and report the blocker. Preserve existing edits and coverage floors.

| Candidate | Decision |
|---|---|
| [Dead or duplicate code](references/code-smells.md) | Check the candidate's removal bar and preserve supported consumer behavior. |
| [Comments, instructions or configuration](references/text-config-hygiene.md) | Keep text and configuration with a parser, runtime or external consumer until its contract deliberately changes. For AI-writing tells, chat residue and placeholders, use [writing tells](references/writing-tells.md). If cleaner wording changes agent behavior, use `octocode-agentic`. For skill packaging or activation, use `octocode-skills`. |
| [Agent leftovers or defects](references/agent-defects.md) | Report error masking and reachable stubs as [SBAR blockers](references/agent-defects.md#escalation-protocol). Keep these out of removal batches. Redact exposed credentials and report them for rotation. |
| [Weak tests or grader gaming](references/tests.md) | Report confirmed gaming as an SBAR blocker. Preserve the behavioral oracle and coverage floors. Never hide a failure by weakening a check or widening a mask. |

Keep defects requiring behavior changes separate from the cleanup. Use `octocode-engineer` for unresolved design choices.

Report [removals, deferrals and checks](output.md), including baseline failures and unrun checks. Finish when the diff contains only authorized changes and the affected checks support preserved behavior, or identify what blocked verification. Run the [renderer](scripts/render-report.mjs) for requested HTML.
