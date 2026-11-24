---
name: octocode-clean-agentic-code
description: "Load this skill first when asked to remove dead code, duplicate helpers, debug leftovers and unused agent instructions or config. Check consumers and verify that behavior survives each removal."
---

# Octocode clean agentic code

1. Record the requested scope, existing edits and affected checks before cleanup. Save each affected check's baseline report (JUnit XML, TAP, JSON, or a list of failing check names) and coverage report under `<workspace>/.octocode/tmp/octocode-clean-agentic-code/`; mark a blocked baseline.
2. List literal candidates with [scan-signals](scripts/scan-signals.mjs): `node <skill-folder>/scripts/scan-signals.mjs [paths] --diff <base>` for the change (omit `--diff` to audit whole paths). Judge each JSON-line candidate with the reference below that names its `signal`; exit 1 only means rows were printed.
3. For each candidate, check semantic references, public entry points, configuration, dynamic loading and external consumers. Use Octocode `lspSearch` references and exact `localSearch` reads. An empty search alone cannot justify deletion.
4. Keep live contracts. Defer uncertain removals with the check that would settle them. Group proven removals into reversible batches, update their callers and regenerate derived files at their source. Flag authentication, authorization, deletion, billing and public API changes for human review. Explain any rewrite beyond the candidates, or a file where `git diff --numstat` shows more added than deleted lines (an untracked file counts as all added).
5. Replay the affected checks and inspect the diff, then compare with the baseline: `node <skill-folder>/scripts/check-diff.mjs <baseline> <after>` ([check-diff](scripts/check-diff.mjs)). Exit 1 means `introduced` holds failures your batch caused: revise the diagnosis or undo only your batch and report the blocker. Report `missing` checks as unverified; other fields: `--help`. Preserve existing edits and coverage floors.

| Candidate | Decision |
|---|---|
| [Dead or duplicate code](references/code-smells.md) | Check the candidate's removal bar and preserve supported consumer behavior. |
| [Comments, instructions or configuration](references/text-config-hygiene.md) | Keep text and configuration with a parser, runtime or external consumer until its contract deliberately changes. For AI-writing tells, chat residue and placeholders, use [writing tells](references/writing-tells.md). If cleaner wording changes agent behavior, use `octocode-agentic`. For skill packaging or activation, use `octocode-skills`. |
| [Agent leftovers or defects](references/agent-defects.md) | Report error masking and reachable stubs as [SBAR blockers](references/agent-defects.md#escalation-protocol). Keep these out of removal batches. Redact exposed credentials and report them for rotation. |
| [Weak tests or grader gaming](references/tests.md) | Report confirmed gaming as an SBAR blocker. Preserve the behavioral oracle and coverage floors. Never hide a failure by weakening a check or widening a mask. |

Keep defects requiring behavior changes separate from the cleanup. Use `octocode-engineer` for unresolved design choices.

Report [removals, deferrals and checks](output.md), including baseline failures and unrun checks. Finish when the diff contains only authorized changes and the affected checks support preserved behavior, or identify what blocked verification. Run the [renderer](scripts/render-report.mjs) for requested HTML.
