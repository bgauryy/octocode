---
name: octocode-clean-agentic-code
description: "Use when removing proven dead code, unused exports, duplicate helpers, temporary logs or agent leftovers without changing behavior. Includes instruction and configuration cleanup, and detecting weakened tests."
---

# Octocode clean agentic code

For each candidate in the authorized scope, identify what removal could break. Check semantic references, public entrypoints, configuration, dynamic loading and external consumers. Graph edges, file size, pattern matches and empty searches alone cannot prove redundancy. Use [Octocode discovery and exact reads](references/octocode-usage.md) to trace the deciding evidence.

Keep live code; defer unresolved candidates with the check that would settle them. Remove proven redundancy in reversible, reviewable batches. Keep one owner, update callers and links, and regenerate derived files with their generator.

| Candidate | Decision |
|---|---|
| [Dead or duplicate code](references/code-smells.md) | Check the candidate's removal bar and preserve supported consumer behavior. |
| [Comments, instructions or configuration](references/text-config-hygiene.md) | Keep text and configuration with a parser, runtime or external consumer until its contract deliberately changes. If cleaner wording changes agent behavior, use `octocode-agentic`. For skill packaging or activation, use `octocode-skills`. |
| [Agent leftovers or defects](references/agent-defects.md) | Report error masking and reachable stubs as [SBAR blockers](references/agent-defects.md#escalation-protocol). Keep these out of removal batches. Redact exposed credentials and report them for rotation. |
| [Weak tests or grader gaming](references/tests.md) | Report confirmed gaming as an SBAR blocker. Preserve the behavioral oracle and coverage floors. Never hide a failure by weakening a check or widening a mask. |

Explain bugs or design changes separately. Reuse existing authorization and ask only about effects outside it. For new branches, wrappers or unresolved ownership, use `octocode-engineer`. Coordinate multiple actors with `octocode-workflows`.

Run and read the relevant checks for each batch. If a failure invalidates the removal proof, revise the diagnosis; if no safe repair is established, revert that batch and report an SBAR blocker. Done when supported behavior and guardrails survive the change. Report [removals, deferrals and checks](output.md), including unrun checks; fewer lines alone prove no gain. Run the [renderer](scripts/render-report.mjs) for requested HTML.
